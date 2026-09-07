# 观测站贡献指南

Qianqian 工程观测站是仓库真相的受控投影。以下规则使它保持如此。

---

## 规则

### 1. 不得在 Web 文章中创造架构真理。

观测站解释已被接受的架构。它不创造架构。

### 2. 每个架构页必须带权威来源。

每个架构页必须携带 `<ProvenancePanel>`,链接到权威文档。

### 3. 每个实验主张必须带证据。

定量主张必须指向测试结果、制品或可执行证据。

### 4. 正式图使用已登记的 Mermaid 资产。

权威架构图保存在 `docs/architecture/diagrams/`,并登记于 `docs/architecture/registry.yml`。

当前现实:若干页面仍内嵌简化版、与权威图等价的 Mermaid 图(如 ARCH-001/002/004/005 的核心)。预期契约 —— 页面引用 registry ID、不携带复制的权威源(AR7)—— 已写入文档,但 `docs:verify` **尚未强制**。内嵌 Mermaid 仅对未登记为架构资产的解释性图可接受。

### 5. 冻结图按版本管理,不做语义编辑。

架构变化时,创建新版本(`ARCH-002-v2.mmd`)。不要改动已冻结的 v1。

### 6. GitHub 状态不是语义状态。

Issue OPEN/CLOSED 是工作流状态。观测站状态是:FROZEN、IMPLEMENTED、VALIDATED、CURRENT、NEXT、PLANNED、DEFERRED、HISTORICAL_EVIDENCE、SUPERSEDED。

### 7. 项目状态来自 project-state.ts。

首页、路线图与架构状态 UI 消费同一来源。不允许重复的状态字符串。

### 8. 机器事实优先采信机器制品。

如果制品文件与 issue 文本不一致,以机器制品为准。issue 文本提供解释/溯源。

### 9. Web 文章是投影,不是权威。

权威架构在 `docs/architecture/`。观测站负责整理与解释。

### 10. 新的重大架构必须先通过自己的设计门槛,观测站状态才能改变。

不要因为你实现了某个东西就改页面上的状态徽章。架构门槛必须先通过。

---

## 新增架构页

1. 确认架构已通过设计门槛
2. 在 `docs/architecture/registry.yml` 登记图
3. 在 `docs/architecture/diagrams/` 创建图
4. 创建页面,带 `<ProvenancePanel>` 链接权威来源
5. 加入 `.vitepress/config.ts` 侧栏
6. 运行 `pnpm docs:verify` 检查不变量

---

## 新增实验页

1. 在 `docs/experiments/registry.yml` 登记
2. 遵循标准契约:问题 → 基线 → 假设 → 方法 → 证据 → 结果 → 架构后果
3. 把证据链接到测试/制品
4. 对已测量的事实使用 <ClaimBadge role="evidence" />

---

## 状态词汇

只使用以下状态值:

| 状态 | 含义 |
|------|------|
| FROZEN | 架构边界已被接受 |
| IMPLEMENTED | 代码已合并,证据已核验 |
| VALIDATED | 实验结果已被证据确认 |
| CURRENT | 活跃的架构边界 |
| NEXT | 紧邻的前沿 |
| PLANNED | 设计尚未冻结 |
| DEFERRED | 留待未来考虑 |
| HISTORICAL_EVIDENCE | 作为可选证据保存 |
| SUPERSEDED | 已被更新版本取代 |

不得发明同义词。
