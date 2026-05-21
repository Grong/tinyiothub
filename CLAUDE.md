# TinyIoTHub — Claude Code 指令

> 项目结构、架构规则、命名规范、开发约定等完整内容见 **[AGENTS.md](AGENTS.md)**。本文档仅包含 Claude Code 特定的行为准则和技能路由。

## 行为准则

**1. Think Before Coding** — 先陈述假设，不确定就问。有多种解读时列出选项，有更简单方案时直接说。

**2. Simplicity First** — 只写解决问题的最小代码。不为单次使用创建抽象，不为不可能的场景加错误处理。

**3. Surgical Changes** — 只改必须改的。不"顺便优化"相邻代码、注释、格式。你的改动引入的孤儿代码（import、变量）要清理，但不要删除已有的 dead code。

**4. Goal-Driven Execution** — 把任务转化为可验证目标。"修 bug" → "写复现测试，然后修"。"加校验" → "写无效输入测试，然后实现"。

## 技能路由

当用户请求匹配可用技能时，使用 Skill 工具作为优先操作调用技能，不要直接回答。

- 产品想法/头脑风暴 → `office-hours`
- Bug/错误排查 → `investigate`
- 部署/推送/创建 PR → `ship`
- QA/测试网站 → `qa`
- 代码审查 → `review`
- 架构审查 → `plan-eng-review`
- 设计系统/品牌 → `design-consultation`
- 可视化审查 → `design-review`
- 保存进度 → `checkpoint`
- 代码健康 → `health`
