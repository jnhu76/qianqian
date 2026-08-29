# Scripts

这里的脚本是实验入口，不是最终 production build system。

## 入口

```text
scripts/fetch-ffmpeg            下载并验证 pinned FFmpeg（bench/ffmpeg-pin.json）
scripts/build-profile <p>       按 bench/profiles/<p>.json 构建 + 产出元数据/体积
scripts/probe-necessity [p]     单组件删除实验 → bench/provenance/
scripts/bench-native            fetch + build all + 跑 harness（端到端）
```

`build-profile` 支持 `--profile-file <path>`（供 probe 变体使用）与
`--force`（忽略构建缓存）。

## 产物

每个 profile（`build/<profile>/`，untracked）：

- `configure-args.txt` + `configure_args_hash`（确定性参数与缓存键）
- `build-meta.json`（ffmpeg tag/sha、编译器、平台、qianqian git sha）
- `meta/enabled-components.txt`（从 config*.h 提取的机器事实）
- `size.json`（static libs / linked / stripped / xz / symbols）
- `qn_bench` + `qn_bench.stripped`

基准结果（`bench/results/<run>/`）由 harness 生成，见 `bench/results/README.md`。

## 已删除

`ffmpeg-profile-native.sh`（早期草稿）已由 profile 系统取代。
