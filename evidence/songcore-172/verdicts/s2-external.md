# S2 外部消费者验收判定 — RC 包盲测（open→probe→decode→seek→EOF→close）

- 判定角色: 独立外部消费者验收员（#172 S2-external，独立 subagent 会话）
- 日期: 2026-10-29
- 战场: worktree `C:\Users\fred1\source\qianqian-172`，分支
  `songcore-172/pr-b-windows-release-lane`
- 方法约束: 模拟对 Qianqian 仓库内部结构一无所知的用户；只使用包内
  `include/`、`lib/`、README 声明的 system libraries 与 manifest/licensing
  文件；消费者源码与产物全部放在 `%TEMP%` 下的独立目录，不引用
  `native/build/`、`crates/`、`evidence/` 或任何仓库内部路径。

## 结论

**S2 外部消费者验收 PASS（本验收范围内），fail-closed 全程成立。**
全新独立 C consumer 按 README 声明的 link contract
（`gcc -O2 -Iinclude <c> -Llib -lsongcore -lbcrypt -o out.exe`，显式链接表
恰为 `-lsongcore -lbcrypt`）一次链接成功；真实执行
open→probe→decode→seek(50%)→EOF→close 全序列、逐步校验返回值、exit 0；
链接产物导入表仅 `bcrypt.dll / KERNEL32.dll / msvcrt.dll`，全部落在
README 声明范围内。**未发现任何失败项**：无包外依赖、无未声明系统库、
无缺失 DLL、不需要内部 FFmpeg archive。

唯一异常属 ask 侧身份引用过时（见 §1）：ask 钉住的 SHA256
`b2c222cd…` 是 s2-package 判定 §6 明确作废且未保留副本的初版容器；
本验收实测的是当前由 sidecar + package-summary 双重自证的现行容器
`7b4d0615…`。该差异已如实记录为 blocker（文档级，需 owner 重钉），
不构成包缺陷，也不影响上述消费结论。

## 1. 包身份（实测）

```text
PATH   C:\Users\fred1\source\qianqian-172\evidence\songcore-172\package\
       songcore-v0.2.0-rc.1-windows-x86_64.zip
SHA256 7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415
       （本会话 sha256sum 实测 == 包旁 SHA256SUMS sidecar ==
       package-summary.json container.zip_sha256，三算一致）
BYTES  1,448,965
```

ask 钉住的 `b2c222cd4efa6b0e8f4fc090181a0cf7eb7bbeb9b35c4f5b2ddf07733f223aa7`
与磁盘现行 zip 不符。核对 `verdicts/s2-package.md` §6（行 239）：

> 初版容器（b2c222cd…，1,458,012 B，pwsh 7 构建）由此作废，不另留副本

即 ask 钉的是已被作废的初版容器哈希。现行容器由 `ops/package-rc.ps1`
于 2026-10-29 07:05 重建（package-summary.json `rebuilt: true`、
`previous_zip_sha256: 339c798d…`），V1–V7 全部复跑 PASS（`ok: true`）。
**本验收对象即该现行容器。**

包内完整性：`sha256sum -c metadata/checksums.txt` → 13/13 OK。
包内 `lib/libsongcore.a` sha256 `1f5f8cf8…fd402` == 构建通道产物身份
（package-summary `artifact_sha256`）；`include/songcore.h` sha256
`cf509a5c…78536` == 冻结 ABI v1 头身份。

## 2. 归档依赖审计（未声明依赖检查）

对包内 `lib/libsongcore.a` 用 mingw-w64 binutils 独立审计（工具
`C:\Users\fred1\tools\mingw64\bin`，GCC 16.2.0，机器级工具链，非仓库内）：

```text
nm 全部 undefined 引用            1185（与 package-summary audits.undefined_count 一致）
减去包内成员自定义符号后，
真正外部未解析符号               127（external-unresolved.txt）
```

127 个的归类（全部有 README/manifest 依据，无未声明项）：

| 类别 | 符号例 | 声明处 |
|---|---|---|
| BCrypt 熵路径 | BCryptOpenAlgorithmProvider / BCryptGenRandom / BCryptCloseAlgorithmProvider | README L44–47 明示 REQUIRED `-lbcrypt`；manifest runtime_dependencies.static.bcrypt 同记录 |
| Win32 系统导入 | __imp_CloseHandle、__imp_MapViewOfFile、__imp_InitOnceBeginInitialize 等 | README L49–50 "Win32 system imports"（随链接工具链） |
| msvcrt 族 CRT | __imp___acrt_iob_func、__imp__aligned_malloc、malloc/memcpy/sin/cos 等 | README L49 "msvcrt-family CRT" |
| mingw 运行时 | __mingw_snprintf、__mingw_vsnprintf、___chkstk_ms | README L49 "provided by the toolchain that links the consumer" |
| POSIX 名（FFmpeg 源） | close、usleep、mkstemp、gettimeofday、sincos 等 | 由工具链静态解析，最终导入表无对应 DLL（见 §4） |

真正外部符号中**没有任何** avcodec/avutil/swresample/zlib/mp3lame 等
外置编解码库符号 —— 与 README "FFmpeg ships in object form inside it"
一致；不需要内部 FFmpeg archive，链接一次成功即为直接证据。

## 3. 全新独立 consumer（源码在临时目录）

`C:\Users\fred1\AppData\Local\Temp\songcore-external-accept-20261029\s2-external-consumer.c`
—— 仅按包内 `include/songcore.h`（ABI v1）从零编写，只用 C 标准库 +
host IO 三回调；逐步校验：abi==1、open 返回 SONG_OK 且 handle 非空、
probe 快照健全（rate/channels>0、duration>0、codec/container 非空、
streams≥1）、decode 循环 EOF 形状（SONG_EOF 且 got==0）、SONG_OK 时
0<got≤4096、seek 返回 SONG_OK、post-seek 首读 SONG_OK 且 got>0、
close（含 song_close(NULL) 文档化 no-op 不崩溃）。

媒体输入（外部用户自备，本会话用机器级 ffmpeg 生成，sha256
`9a46f50b905e06bd77819bb946247ab7d3ed8167f7550c5d7831137a742594a4`）：

```text
C:\Users\fred1\AppData\Local\Temp\songcore-external-accept-20261029\sample10s.flac
10.000 s，44100 Hz，立体声，flac（ffprobe 复核）
```

## 4. 实测序列（consumer 真实输出，exit 0）

```text
stage abi:   version=1 OK
stage open:  SONG_OK, handle=000002dcf4562370
stage probe: codec=flac container=flac rate=44100 ch=2 mask=0x3 bps=16
             duration_us=10000000 streams=1 selected=0 OK
stage decode: 441000 frames to SONG_EOF (got==0 at EOF) OK
             （441000 = 10.000 s × 44100，帧数与容器时长精确一致）
stage seek:  target_us=5000000 landing_us=4911020 OK
             （50% 请求；落点 ~4.911 s，处于头文件声明的 codec 帧容差内；
              header 明示 seek 不承诺 sample-exact）
stage eof:   clean SONG_EOF terminal after 224424 post-seek frames OK
             （224424 ≈ 5.089 s 剩余，与落点一致）
stage close: completed OK
S2-CONSUMER PASS   EXIT=0
```

链接（README 契约原文，一次通过）：

```text
gcc -O2 -Iinclude ../s2-external-consumer.c -Llib -lsongcore -lbcrypt \
    -o ../s2-external-consumer.exe    → LINK_OK
```

导入表审计（x86_64 objdump -p 实测，验收决定性证据）：

```text
DLL Name: bcrypt.dll      ← README 显式声明（-lbcrypt REQUIRED）
DLL Name: KERNEL32.dll    ← Win32 system imports（声明内）
DLL Name: msvcrt.dll      ← msvcrt 族 CRT（声明内）
```

无 libwinpthread-1.dll、无 libgcc_s_seh-1.dll、无 zlib1.dll、无任何
av* DLL。干净 PATH（仅 `C:\Windows\System32`）运行复测 exit 0
——无缺失 DLL、无工具链 DLL 运行时依赖。

包内自带 smoke（README "Consumer smoke" 同一链接行）复跑亦 PASS
（`external-consumer-example: PASS`，exit 0），与自写 consumer 结果一致。

## 5. 可重放入口（已提交）

`ops/verify-external.ps1`（战场分支提交）：解压→包内校验→写嵌入式
consumer→生成/指定媒体→README 契约编译→运行六步序列→导入表白名单
审计，全程 fail-closed，stdout 输出 JSON 摘要，日志落
`evidence/songcore-172/external-accept/<stamp>-external-log.txt`（该目录为
`ops/verify-external.ps1` 的本地输出，2026-11-01 后 git-ignored）。

实测：

```text
正向: powershell -File ops/verify-external.ps1          → 全链 PASS，exit 0
      （临时目录 songcore-external-verify-20261029-071806，
       13/13 checksums OK，序列 PASS，imports = bcrypt/KERNEL32/msvcrt）
负向: 同脚本 -ExpectedZipSha256 000…000                  → FAIL [zip-identity]
      fail-closed，exit 1（门真实可拒）
```

前置（机器级，非仓库级）：mingw-w64 gcc+binutils（默认探测
`C:\Users\fred1\tools\mingw64\bin`，可 -GccDir 覆盖或走 PATH）、
ffmpeg（仅用于默认媒体生成；外部用户可 -MediaFile 传入自己的媒体）。

## 6. 失败项清单

按 ask 枚举的失败类别逐一核对，**全部为空**：

```text
包外依赖:            未发现（§2、§4 导入表）
未声明系统库:        未发现（bcrypt 已在 README L37/L39/L44–47 显式声明——
                     ask 的"BCrypt 未在 README 声明"假设与包内事实不符）
缺失 DLL:            未发现（干净 PATH 复测 exit 0）
需要内部 FFmpeg archive: 不需要（外部符号零 av*；README 契约链接一次成功）
```

## 7. Blocker（唯一，文档级）

ask 钉住的容器 SHA256 `b2c222cd…` 指向 s2-package 判定已作废且未保留
副本的初版容器；现行容器为 `7b4d0615…`（sidecar + summary 双自证）。
本验收以现行容器为对象完成并如实记录。需 owner 重钉 ask/验收引用链，
使 ask 身份引用与 `SHA256SUMS`/`package-summary.json` 一致。除此之外
无 blocker。

## 8. 证据清单

```text
C:\Users\fred1\source\qianqian-172\ops\verify-external.ps1            （提交）
C:\Users\fred1\source\qianqian-172\evidence\songcore-172\verdicts\s2-external.md（本文件）
C:\Users\fred1\source\qianqian-172\evidence\songcore-172\external-accept\20261029-071806-external-log.txt（重放日志）
C:\Users\fred1\AppData\Local\Temp\songcore-external-accept-20261029\s2-external-consumer.c
C:\Users\fred1\AppData\Local\Temp\songcore-external-accept-20261029\s2-external-consumer.exe
C:\Users\fred1\AppData\Local\Temp\songcore-external-accept-20261029\external-unresolved.txt（127 外部符号清单）
C:\Users\fred1\AppData\Local\Temp\songcore-external-accept-20261029\sample10s.flac（媒体输入）
```

本判定只覆盖 windows-x86_64 静态消费路径的上述实测范围；不推广到其他
target、共享库形态或发布级打包验收（那些由各自通道判定负责）。
