# research

## What is this directory?

研究记录：研究了什么、怎么测、证据是什么、对决策的影响。

- **Research is evidence**：可以影响决策，但不会自动成为 current
  architecture authority。
- **Negative result is valid**：被否决的变体同样是结论。
- 被接受的 durable 决策提炼为 [../adr/](../adr/)；current architecture
  truth 由 [../architecture/](../architecture/) 拥有。
- 每份 research 文档标注 `Status: active | concluded | superseded` 与
  `Decision authority: no`。

## What belongs here?

- [ffmpeg-minimization.md](ffmpeg-minimization.md) — 裁剪方法的实测证据。
- [wasm.md](wasm.md) — WASM 可行性测量（44 fixtures，多 runtime）。

## What does not belong here?

- 当前架构描述、normative 契约、phase 施工记录。

## What should I read first?

新研究用 [../standards/templates/research.md](../standards/templates/research.md)
模板；既有证据按问题在上表选择。
