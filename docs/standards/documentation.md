# Qianqian documentation standard

> Authority: Normative（工程标准，覆盖所有长期仓库 Markdown）
> Scope: 文档分类法、权威模型、Markdown house style、canonical templates、AGENTS 继承模型

仓库自身的规则必须能被任何人或任何 coding agent 仅从 repository 内发现，不依赖
agent memory、外部 prompt 或某次聊天上下文。

> A document's structure is determined by its document class, not by the
> author or AI that created it.

`.github/` 下的 Issue / PR 模板是 contribution forms，不属于任何 document
class，不受 house style 的 heading 约束。

---

## 文档分类法（taxonomy）

长期文档只有以下类别。每类有固定位置、固定 ownership、固定 non-purpose。

| Class | Location | 回答的问题 | Non-purpose |
|---|---|---|---|
| Repository entry | `README.md` | Qianqian 是什么、能做什么、30 秒架构、快速开始、repo map | 不是完整 architecture manual |
| Agent governance | `AGENTS.md`（root / local） | AI agent 在本仓库如何工作、如何选权威 | 不装 feature 手册、campaign 状态、完整命令百科 |
| Shared vocabulary | `CONTEXT.md` | 稳定术语 + repository mental model | 不是 architecture / workflow authority |
| Contribution entry | `CONTRIBUTING.md` | human contributor 从哪里开始 | 不是另一份 development manual，只做 router |
| Product | `docs/product/` | 产品是什么、用户行为应该是什么 | 不是 native implementation authority |
| Architecture | `docs/architecture/` | 当前系统怎样组成和运行（current semantic truth） | 不写施工故事（“PR #22 中我们先……”） |
| Contract | `docs/contracts/` | normative machine / cross-layer 契约（API、ABI、错误模型、FFI 边界） | 只有 contract 可系统性使用 MUST / MUST NOT / SHOULD / MAY |
| Standard | `docs/standards/` | 长期工程政策（documentation、testing、native、kotlin、code-quality） | 不描述某个具体组件 |
| Development | `docs/development/` | 安装、构建、发布等 operational how-to | 不承载架构语义或契约 |
| Research | `docs/research/` | 研究了什么、怎么测、证据是什么、对决策的影响 | **不是** current architecture authority |
| ADR | `docs/adr/` | 为什么最终做出某项决策 | 不与 experiment report 混同 |
| Archive | `docs/archive/` | 历史：`experiments/`、`closeouts/`、`superseded/` | 不得作为 current behavior authority |
| Directory readme | 各目录 `README.md` | 该目录是什么、放什么、不放什么、先读什么 | 不复制其他文档正文 |

当前物理布局 ≠ 目标布局：`docs/` 顶层仍有多份按上表应归属子目录的文档，它们的
迁移由 **DOCS-IA-2** 独立执行；在迁移前，按内容语义使用本表判断权威。

---

## 权威模型（fact-type authority）

不采用全局线性排名。不同类型的事实有不同的 canonical authority：

| Fact | Authority |
|---|---|
| Agent 工作规则 | root / local `AGENTS.md` |
| 稳定词汇 | `CONTEXT.md` |
| 产品语义 | `docs/product/` |
| 当前架构 | `docs/architecture/` |
| Normative API / 行为 | `docs/contracts/` |
| 工程政策 | `docs/standards/` |
| Setup / workflow | `docs/development/` |
| 实验证据 | `docs/research/` |
| 架构决策理由 | `docs/adr/` |
| 历史 / superseded 材料 | `docs/archive/` |
| 文档与 code/tests 冲突时 | 触发 reality audit，不静默站队 |

### 冲突解决

当 permanent doc 与当前 code/tests 不一致时，禁止：默默选一边、自动改 contract、
自动认定 code 或 docs 永远正确。必须：

1. 识别 fact type；
2. 找到 canonical authority；
3. 检查当前实现 / 证据；
4. 分类为 implementation drift / documentation drift / unresolved ambiguity；
5. 只执行被授权的 corrective；
6. 无法确定权威时 STOP。

---

## Markdown house style

### Headings

- 每份文档 exactly one H1；
- ATX 风格（`#` / `##` / `###`）；
- 不做 section numbering（`## 3. Foo` 这类 §编号不再新增）；
- heading 统一 sentence case；
- 不允许为视觉效果跳 heading level。

### Code blocks

Fenced code block 必须标注 language / type（`c`、`cpp`、`kotlin`、`bash`、
`text`、`json`、`yaml`……），除非确实没有对应类型。

### Markdown over HTML

优先 standard Markdown；禁止仅为了排版引入 HTML。

### Tables

表格只用于真正适合矩阵表达的数据，不把大段 prose 塞进表格。

### Links

优先 repository-relative links；不复制目标文档正文来避免链接。

### One fact, one authority（hard invariant）

> One semantic fact MUST have one canonical authority.

其他文档链接过去。禁止长期复制：ABI symbols、error 定义、architecture 规则、
build 要求、feature 语义。

### Permanent truth vs historical story

`product/`、`architecture/`、`contracts/`、`standards/`、`development/` 描述
**current semantic truth**。只有 `research/`、`adr/`、`archive/` 可以写对比、
实验、prior decision、PR / corrective context、superseded 状态。

### Status metadata

只在确实需要 status 的 document class 使用：Research 用
`Status: active | concluded | superseded`；ADR 用
`Status: proposed | accepted | superseded | rejected`。
不给普通 architecture 文档加易腐烂的 `Status: 80% complete`、
`Current Phase: ...`、`Last PR: #...`。

### Dates

日期对 decision / evidence 有意义时使用 ISO `YYYY-MM-DD`；不作为普通永久文档
的装饰性 metadata。

---

## 新建文档决策树

```text
Need a new Markdown file?
        |
        v
Is it current product behavior?      -> product/
Is it current system structure?      -> architecture/
Is it normative API/behavior?        -> contracts/
Is it engineering policy?            -> standards/
Is it setup/workflow?                -> development/
Is it an experiment/investigation?   -> research/
Is it a decision record?             -> adr/
Is it superseded/history/closure?    -> archive/
Otherwise:
        STOP and justify a new class.
```

---

## Canonical templates

模板位于 `docs/standards/templates/`：

```text
architecture.md      系统组件架构文档
contract.md          normative 契约
research.md          研究记录
adr.md               决策记录
directory-readme.md  目录 README
local-agents.md      子目录 AGENTS
```

规则：

- 模板是 skeleton，直接复制作为新文档起点；
- 每个模板标注 required / optional sections；
- **删除 genuinely non-applicable section，而不是填 “N/A” 制造噪声**；
- 模板不得催生空 section，也不得包含虚构的 Qianqian 内容。

Agent 发现路由：

```text
AGENTS.md
  -> docs/README.md
  -> docs/standards/documentation.md
  -> docs/standards/templates/<document-class>.md
```

在创建任何长期 Markdown 文档之前：先分类，再使用对应 canonical template。

---

## AGENTS 继承模型

```text
root AGENTS.md
        |
        v
nearest applicable local AGENTS.md
```

- root 拥有 repo-wide 规则：mission / scope、work mode、reality-first、
  authority routing、architecture boundaries、change discipline、
  evidence escalation、verification selection、delivery discipline；
- local **extends** root，只写 genuinely local 规则：local scope、
  forbidden dependencies、required authorities、local verification；
- local AGENTS 不得：复制 root 内容、重新定义 global authority、
  复制 architecture contract；
- 没有真正的 local divergence 就不要创建 local AGENTS；
- 长期可能合理的位置：`apps/desktop/AGENTS.md`、`native/AGENTS.md`——目录尚不
  存在时只在本标准记录规则，不预建文件。

---

## AI 文档卫生规则（禁止模式）

- **Status diary**：permanent doc 里写 “PR #22 fixed this / Phase 3 is now
  80%”——禁止，去 archive / issue；
- **Duplicate contract prose**：同一份 ABI / error / architecture 规则在
  README、architecture、research、CONTRIBUTING 各复制一遍——禁止，链接到
  canonical authority；
- **Giant context preload**：要求 “每个任务先读所有 docs”——禁止，按
  `docs/README.md` 路由加载最小相关集合，不要递归通读 `docs/`；
- **Template cargo cult**：为满足模板制造空 section——禁止；
- **Invented future facts**：把尚未实现的 target（如未来目录布局）写成
  current reality——禁止。
