# E06 Status

```text
IMPLEMENTATION: CODE_COMPLETE_PENDING_VALIDATION
BUILD REPLAY:   NOT_EXECUTED
CORPUS GATE:    NOT_EXECUTED
AUDIBLE SMOKE:  NOT_EXECUTED
```

当前分支已实现 importer、Xmake replay target、production SongCore、PCM pipe、自动等价验证脚本和 test-only audible sink。

由于本次 GitHub 施工环境没有可执行的 Xmake/真实音频设备，本文件明确不把 code-complete 写成 PASS。

本地验证完成后，用 `build/ffmpeg-xmake/verify/report.json` 的真实数据更新 E06 主文档/PR，不提交本机 build tree。
