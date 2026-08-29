# Scripts

这里的脚本是实验入口，不是最终 production build system。

## `ffmpeg-profile-native.sh`

Stage A 的初始 FFmpeg allow-list。

目标不是长期维护手写 configure 参数，而是最终得到：

```text
enabled-components.txt
component → corpus evidence
```

后续应增加：

- reproducible build；
- size report；
- symbol report；
- benchmark runner；
- corpus validator；
- WASM profile。
