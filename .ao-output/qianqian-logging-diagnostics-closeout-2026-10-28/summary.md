# Logging / Diagnostics 验证交付

共执行 5 个步骤。结果为限定范围验证交付，#296 尚未完成关闭验收。

48 个故障控制格：22 PASS、3 KNOWN_RESIDUAL、11 NOT_APPLICABLE、12 NOT_RUN。32 个真实子进程控制、23 个精确 mutation RED、2 个 harness 负控制和 11 类字节恢复收据在 evidence/diagnostics-tier2-296/ 下。

WASAPI #8/#9/#10 和 critical α hook 的四控制通过。exporter disable/flush 自诊断与 host exit report 的适用格通过。non-α previous-hook 保留同步残余。native FFmpeg 与 decode read/seek 缺少已核验工件，保持 NOT_RUN。FFmpeg unset-discard 无 sink/queue 的四故障机制不适用，其实际安装运行仍未验证。

发现 FFMPEG_LOG=0 的 Rust/native 配置门不一致，源码层归 FAIL_PRODUCT，原生运行影响 NOT_RUN；只交付独立修复提案。初版 oracle 修订和 Windows 既有 CRLF source-mutation FAIL_ORACLE 均保留原始证据。

Linux 原完整 pre-push gate 8 项通过、退出码 0；Windows 专项 Clippy 通过，完整 workspace 的两个既有 oracle 失败没有计作 PASS。独立审查接受限定交付，不接受关闭 #296。

#290 已完成现状刷新和 scoped 关闭建议；原生 Windows broken-sink、terminal sink 等未执行项及 T1–T4 保留。#317 可由 owner 按已交付审计记录关闭。最终 PR 链接由当前聊天的交付消息提供。
