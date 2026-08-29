# Scripts

这里的 `scripts/` 主要是 Phase 0 benchmark / corpus 实验入口，不是长期 production build system。

## Step 1 benchmark 入口

```text
scripts/fetch-ffmpeg            下载并验证 pinned FFmpeg（bench/ffmpeg-pin.json）
scripts/build-profile <p>       按 bench/profiles/<p>.json 构建 + 产出元数据/体积
scripts/probe-necessity [p]     单组件删除实验 → bench/provenance/
scripts/bench-native            fetch + build all + 跑 harness（端到端）
```

`build-profile` 支持 `--profile-file <path>`（供 probe 变体使用）与
`--force`（忽略构建缓存）。

## Step 1.5 selective-build 入口

候选 production build boundary 从 E06 开始由仓库根目录的 `xmake.lua` 接管：

```text
xmake ffmpeg-import             升级/import 时让 upstream configure+Make 求一次真实 closure
xmake qianqian_av               normal build 直接重放 closure → libqianqian_av.a
xmake qn_pcm_dump               同时构建 SongCore + 极薄 PCM pipe
python3 tools/verify_xmake_core.py
python3 tools/play_smoke.py <mp3>
```

`tools/ffmpeg_import.py` **不是第二套 FFmpeg dependency resolver**。它观察 upstream `V=1` 的真实 compiler invocation 并生成 untracked manifest；之后 Xmake normal build 不再调用 FFmpeg Makefile。

`tools/play_smoke.py` 的 `sounddevice` 只是 acceptance audio sink，不进入 shipping dependency graph，也不参与 MP3 解码。

## Step 1 benchmark 产物

每个 profile（`build/<profile>/`，untracked）：

- `configure-args.txt` + `configure_args_hash`（确定性参数与缓存键）
- `build-meta.json`（ffmpeg tag/sha、编译器、平台、qianqian git sha）
- `meta/enabled-components.txt`（从 config*.h 提取的机器事实）
- `size.json`（static libs / linked / stripped / xz / symbols）
- `qn_bench` + `qn_bench.stripped`

基准结果（`bench/results/<run>/`）由 harness 生成，见 `bench/results/README.md`。

## 已删除

`ffmpeg-profile-native.sh`（早期草稿）已由 profile 系统取代。
