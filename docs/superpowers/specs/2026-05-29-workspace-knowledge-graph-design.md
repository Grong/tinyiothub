# 工作区知识图谱 — 设计文档

> 替代现有 workspace_resources 文件管理器，升级为文档驱动的知识图谱系统。

**目标**：让 AI Agent 理解工作区的空间结构、设备关系和功能要素，能回答「机房 A 的温湿度传感器是什么？」这类需要上下文推理的问题。

**核心原则**：用户只写自然语言 Markdown 文档，AI 自动提取实体和关系构建图谱。用户不需要手动创建节点、画边、填属性。

---

## 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 主要使用者 | AI Agent | 图谱为 Agent 提供结构化工作区上下文 |
| 知识来源 | 混合模式 | 人类建核心框架，AI 提取和补充细节 |
| Schema 灵活性 | 核心固定 + 可扩展 | 核心类型有内置支持，自定义类型自由扩展 |
| 与现有资源的关系 | 完全替代 | 旧资源数据迁移到新模型，文件作为实体附件 |
| 用户交互范式 | 文档驱动 | 用户维护 Markdown 文档，AI 自动提取实体/关系 |

---

## 数据模型

三层结构：**文档（Source of Truth）→ 实体（AI 提取的节点）→ 关系（AI 提取的边）**。

### KnowledgeDocument（知识文档）

用户维护的核心内容，Markdown 格式。一切知识的入口。

```rust
struct KnowledgeDocument {
    id: String,
    workspace_id: String,
    title: String,
    content: String,            // Markdown 正文
    tags: Vec<String>,          // AI 生成 + 手动添加，用于快速检索
    parse_status: ParseStatus,  // pending | parsed | failed
    created_at: String,
    updated_at: String,
}
```

### KnowledgeEntity（知识实体）

AI 从文档中提取的结构化节点。

```rust
struct KnowledgeEntity {
    id: String,
    workspace_id: String,
    source_document_id: String, // 来源文档
    entity_type: String,        // 核心类型或 custom:xxx
    name: String,
    description: Option<String>,// 实体的自然语言描述
    properties: String,         // JSON，如 {"面积": "50㎡", "位置": "B1层"}
    tags: Vec<String>,          // AI 生成的标签，用于语义检索
    file_ids: Vec<String>,      // 关联的附件文件引用（存储路径，同现有上传机制）
    confidence: f32,            // AI 提取置信度 0-1
    created_at: String,
    updated_at: String,
}
```

**核心实体类型（内置）：**

| 类型 | 说明 | 标签示例 |
|------|------|---------|
| `space` | 空间场所 | `#建筑` `#楼层` `#机房` `#园区` |
| `device` | 设备/传感器 | `#网关` `#传感器` `#温湿度` `#门禁` |
| `functional` | 功能要素 | `#消防` `#逃生路线` `#供电` `#安防` |
| `custom:xxx` | 自定义扩展 | `custom:大棚` `custom:灌溉区` |

### KnowledgeRelation（关系）

AI 提取的实体间关系。

```rust
struct KnowledgeRelation {
    id: String,
    workspace_id: String,
    source_entity_id: String,
    target_entity_id: String,
    relation_type: String,      // 核心关系或 custom:xxx
    properties: String,         // JSON，附加信息如 {"楼层": "3-5"}
    confidence: f32,
}
```

**核心关系类型（内置）：**

| 类型 | 方向 | 示例 |
|------|------|------|
| `contains` | 空间 → 空间/设备 | 机房 A `contains` 温湿度传感器 |
| `manages` | 网关 → 终端 | GW-01 `manages` 终端设备 01 |
| `monitors` | 传感器 → 空间 | 温湿度传感器 `monitors` 机房 A |
| `references` | 实体 → 文档 | 逃生路线图 `references` 3 号楼 |
| `connects_to` | 设备 → 设备 | 网关 `connects_to` 交换机 |
| `custom:xxx` | 任意 | 自由扩展 |

### 为什么用标签而不是枚举类型来做分类

- **多维度**：一个实体可以同时标记 `#关键设施` `#高风险` `#温湿度`，枚举做不到
- **AI 友好**：LLM 生成标签比分类到固定枚举更自然，检索时标签匹配语义化更强
- **渐进细化**：标签随时新增不改变 Schema
- **快速检索**：Agent 查询「消防相关的知识」→ 直接匹配 `#消防` `#逃生` 标签，不用遍历整个实体表

### 数据库表结构

```sql
-- 替代旧 workspace_resources 表（旧表保留做兼容）
CREATE TABLE knowledge_documents (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    parse_status TEXT NOT NULL DEFAULT 'pending',  -- pending | parsed | failed
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE knowledge_entities (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    source_document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    properties TEXT NOT NULL DEFAULT '{}',
    tags TEXT NOT NULL DEFAULT '[]',
    file_ids TEXT NOT NULL DEFAULT '[]',
    confidence REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE knowledge_relations (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    source_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    target_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',
    confidence REAL NOT NULL DEFAULT 0
);
```

---

## 用户工作流

### 核心流程

```
创建/编辑知识文档 → [保存并解析] → AI 提取实体和关系
                                         │
                                    ┌────┴────┐
                                    ▼         ▼
                              人工确认/修正   自动注入 Agent 上下文
                                    │
                                    ▼
                           文档变更时 → [重新解析]
```

### 用户操作 vs 系统操作

| 用户做的 | 系统自动做的 |
|---------|------------|
| 写 Markdown 文档 | AI 解析提取实体和关系 |
| 上传附件（拖拽） | AI 生成文档和实体标签 |
| 确认/修正 AI 提取结果 | 注入结构化上下文到 Agent |
| 添加/修改标签 | 文档变更后自动重新解析 |
| 删除/归档文档 | 级联清理关联的实体和关系 |

用户**不需要**：手动创建节点、画边、填 JSON 属性。

### Markdown 文档示例

用户写的文档：

```markdown
## 园区概况
阳光科技园区位于深圳南山，占地 5 万㎡，包含 3 栋建筑。

## 空间结构
- 1 号楼 1-2 层：商业配套（商场、餐饮）
- 2 号楼 3-18 层：办公区
- 3 号楼：数据中心
  - 机房 A：含温湿度传感器和门禁控制器

## 设备清单
- 2 号楼每层一个网关（GW-2F-01 ~ GW-2F-16），管理楼层内所有 IoT 终端
- 3 号楼机房 A：
  - TH-A-01：温湿度传感器，监控机房环境
  - AC-A-01：门禁控制器，管理机房出入

## 消防系统
[消防逃生路线图.png] 覆盖全园区，每层设有消防栓和逃生指示牌
```

AI 提取结果：

```
实体 (12 个):
  阳光科技园区 (space) #园区 #南山
  ├── 1 号楼 (space) #商业
  ├── 2 号楼 (space) #办公
  │   ├── GW-2F-01 (device) #网关 #2号楼
  │   └── ...（16 个网关）
  └── 3 号楼 (space) #数据中心
      └── 机房 A (space) #机房
          ├── TH-A-01 (device) #传感器 #温湿度
          └── AC-A-01 (device) #门禁 #安防

关系 (15+ 条):
  contains(园区, 1号楼)
  contains(2号楼, GW-2F-01)
  monitors(TH-A-01, 机房A)
  ...

附件:
  消防逃生路线图.png
```

---

## UI 设计

### 整体布局

从现有「资源卡片网格」改为「知识文档列表」：

- **顶部**：搜索框 + 标签筛选（`#空间` `#设备` `#消防`）+ 新建按钮
- **列表**：文档卡片，显示标题、标签、解析状态（✅已解析 / ⏳待解析）、实体数/关系数/附件数、更新时间
- **空状态**：引导创建第一篇知识文档

### 文档编辑器（弹窗/侧面板）

- Markdown 编辑区（支持预览）
- 标签输入区（芯片输入，同现有标签组件）
- 附件上传区（拖拽上传，同现有组件）
- **AI 解析结果面板**（可折叠）:
  - 按置信度排序的实体列表
  - 每条实体显示类型图标、名称、属性摘要、标签
  - ✅ 确认 / ✏️ 编辑按钮，逐条审核
  - 关系列表（source → type → target）
  - [全部确认] [重新解析] 批量操作按钮
- 底部：标签提示「Ctrl+Enter 提交」+ [保存并解析] 按钮

### 状态说明

- **待解析**：新创建或内容已修改但未重新解析
- **已解析**：AI 已成功提取，结果经人工确认
- **解析失败**：显示错误信息，用户可修改文档后重试

---

## Agent 集成

### 上下文注入策略

Agent 每次对话开始时，系统向其 System Prompt 注入工作区知识：

**1. 摘要（~200 tokens）**

```
[工作区知识摘要]
阳光科技园区，占地 5 万㎡，位于深圳南山。
3 栋建筑，12 个空间实体，8 个设备，2 份关联文档。
标签: #园区介绍 #空间布局 #设备清单
```

**2. 结构树（~500 tokens）**

```
园区
├── 1 号楼 (商业, 楼层 1-2)
├── 2 号楼 (办公, 楼层 3-18)
│   └── GW-2F-01 ~ GW-2F-16 [网关]
└── 3 号楼 (数据中心)
    └── 机房 A
        ├── TH-A-01 [温湿度传感器, id:xxx]
        └── AC-A-01 [门禁控制器, id:xxx]
```

**3. 检索工具**：Agent 可调用 `search_knowledge(query, tags)` 在对话中动态检索更多细节。

注入格式为 Markdown 树形文本（非 JSON），因为 LLM 对树形文本的理解效果最好且 token 效率最高。

### 上下文注入位置

在 `chat/stream` 的 `system_prompt` 参数之前拼接知识上下文：

```
[系统角色 Prompt]
...
[工作区知识上下文]  ← 在此注入
...
[用户消息]
```

通过 `GET /workspaces/{id}/knowledge/context` 获取注入内容，Agent 服务在构建 system_prompt 时调用。

---

## API 设计

| 端点 | 方法 | 说明 |
|------|------|------|
| `/workspaces/{id}/knowledge/documents` | GET | 文档列表（支持 `?q=&tags=&status=` 筛选） |
| `/workspaces/{id}/knowledge/documents` | POST | 创建文档 |
| `/workspaces/{id}/knowledge/documents/{did}` | GET | 获取文档详情 |
| `/workspaces/{id}/knowledge/documents/{did}` | PUT | 更新文档 |
| `/workspaces/{id}/knowledge/documents/{did}` | DELETE | 删除文档（级联删除实体和关系） |
| `/workspaces/{id}/knowledge/documents/{did}/parse` | POST | 触发 AI 解析，返回提取的实体和关系 |
| `/workspaces/{id}/knowledge/entities` | GET | 实体列表（支持 `?type=&tags=` 筛选） |
| `/workspaces/{id}/knowledge/entities/{eid}` | PUT | 手动修正实体 |
| `/workspaces/{id}/knowledge/relations` | GET | 关系列表 |
| `/workspaces/{id}/knowledge/search` | GET | 全文检索（`?q=&tags=&type=`） |
| `/workspaces/{id}/knowledge/context` | GET | 获取 Agent 上下文注入文本 |
| `/workspaces/{id}/knowledge/files/upload` | POST | 上传附件（拖拽，同现有 `apiUpload`） |

### 关键 API 详解

**POST `/knowledge/documents/{did}/parse`**

```
1. 读取文档 Markdown 内容
2. 调用 LLM（zeroclaw MiniMax provider）提取实体和关系
3. 写入 knowledge_entities + knowledge_relations 表
4. 同时生成文档和实体标签
5. 更新文档 parse_status = "parsed"
6. 返回完整提取结果供前端展示面板
```

LLM Prompt 设计要点：
- 提供实体类型定义和标签规范
- 要求返回结构化 JSON（实体数组 + 关系数组）
- 每条实体/关系附带置信度
- 温度参数 0.3（低温度保证一致性）

**GET `/knowledge/context`**

```
1. 获取 parsed 状态的所有实体和关系
2. 生成摘要文本 + 树形结构文本
3. 返回纯文本（直接拼接进 system_prompt）
```

---

## 迁移策略

旧 `workspace_resources` 表保留，新功能操作新表。两套系统在过渡期共存：

- 新知识图谱路由：`/workspaces/{id}/knowledge/*`
- 旧资源路由保持：`/workspaces/{id}/resources/*`

旧数据迁移为可选操作：
- `resource_type=document` → 转为 `KnowledgeDocument`，文件作为附件
- `resource_type=image/scene/device_model` → 作为附件追加到相关文档
- 迁移脚本为一次性操作，不自动触发

---

## 实现范围

### 不在此设计范围内的

- 知识图谱可视化画布（节点-边图渲染）。当前阶段 Agent 消费和列表 UI 已满足需求，可视化可在后续版本考虑
- 多文档之间的交叉引用自动合并
- 知识版本历史和差异对比
- 设备实时数据与知识图谱的自动同步（如设备状态变更 → 更新实体属性）
