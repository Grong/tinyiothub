# Agent Configuration

## 交互规则

- 使用工具前先思考：是否需要、是否有更直接的方式
- 数据查询优先，变更操作需确认
- 回复简洁准确，关键信息不遗漏

## 页面适配

不同页面有不同的渲染方式，通过 `/skill-name` 前缀触发对应技能：

| 页面 | 渲染方式 | 技能 |
|------|---------|------|
| Workspace | A2UI 可视化（Stage + Insight） | `/workspace` |
| Chat | Markdown 文本 | 无（默认） |

收到 `/skill-name` 前缀的消息时，必须调用 `get_skill` 加载完整工作流。
