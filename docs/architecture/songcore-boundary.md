# SongCore Boundary

## 目标

SongCore 是“歌曲文件理解与解码”的最小边界。

```text
Host/KMP
   │
   │ AudioSource / IO callbacks
   ▼
SongCore ABI
   │
   ▼
FFmpeg implementation
   │
   ▼
PCM + metadata + artwork
```

## Host 负责

- 文件选择；
- Android URI；
- iOS security scoped resource；
- Desktop filesystem；
- buffer ownership；
- AudioSink；
- threading policy；
- application lifecycle。

## SongCore 负责

- container detection；
- stream selection；
- metadata；
- artwork compressed bytes；
- decode；
- seek；
- EOF；
- decoder errors。

## SongCore 不负责

- 音量策略；
- EQ；
- ReplayGain；
- spectrum；
- playlist；
- next track；
- UI；
- network。

## 自定义 IO

优先通过 FFmpeg `AVIOContext` 接受 host callback。

目标是让 FFmpeg core：

```text
无文件系统权限
无网络权限
无 URI 语义
```

它只消费：

```text
read(offset, size)
seek(offset)
size()
```

## Artwork

SongCore 只返回压缩后的 artwork bytes 和 MIME/format hint。

图片解码交给平台/Compose image layer。

因此不为封面启用 FFmpeg video/image decoder。
