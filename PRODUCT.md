# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

工业 IoT 开发者与运维团队并重（已确认）：

- **开发者/集成商**：构建物模型、编写与匹配设备驱动、配置规则与定时任务、通过 MCP 把平台接入 Claude Desktop / Cursor 等工具链。
- **运维团队**：在日常运维中使用 AI 巡检、告警闭环、自然语言排障与 A2UI 动态仪表盘，把处置工作交给自治 Agent。

次要场景：私有化部署的工厂/楼宇现场，资源受限的边缘环境。

## Product Purpose

AI 时代的万物互联底座：以「物」（设备、空间、产线、楼宇）为本体的 AIoT 平台。核心承诺是「万物智能只需三步：① 构建物 ② 通过驱动更新实时数据 ③ 剩下的交给 AI」。

成功意味着：设备接入后，监控、巡检、故障诊断与恢复由 AI 自治完成，人只在需要审批与决策时介入。项目处于 v0.x 早期阶段，核心平台（物模型、告警、事件、AI Agent、MCP）可用，API 在 1.0 前可能有破坏性变更。

## Positioning

**AI 自治是唯一核心差异化（已确认）**：Thing Agent Loop（AI 被设备事件/定时巡检/用户指令唤醒，查本体、做决策、经三态策略门 off/diagnose/act 自主操作设备，行动后回读验证并留全量审计）、A2UI 实时渲染仪表盘、内嵌 MCP Server。轻量单进程部署（~80MB、SQLite 零依赖）是实现手段，使 AI 自治能落地到私有化与边缘场景，但它本身不是定位。

邻近产品（ThingsBoard 等）无法诚实复制的 claim：设备事件的「感知→决策→行动→验证」全链路由平台内生的 AI Agent 闭环完成，而不是外挂的告警通知。

## Operating Context

- 单进程私有化部署：二进制 + 内置 SQLite，无外部数据库依赖；Docker 多架构镜像（Docker Hub `grong/tinyiothub`）。
- 边缘网关（apps/edge）通过 MQTT 上报数据，主题契约见 README「MQTT 主题」与 `docs/device-event-contract.md`。
- 开发者通过 Web 管理界面、REST API（`/api/v1/`，统一 `{code, msg, result}` 响应）、MCP 端点（`/mcp`）三条路径使用平台。
- 运维日常：AI 聊天（SSE 流式 + A2UI 动态 UI）、告警与通知渠道、心跳巡检、Cron 定时任务，均按 Workspace 隔离。
- 部署目标：Linux、Windows；HarmonyOS 实验性。

## Capabilities and Constraints

**已可用**：物本体模型（层级树、属性/事件/操作、知识文档）、MQTT 驱动、物模板（DTDL/WoT 导入、DTDL 导出）、L0–L3 分级自愈引擎、规则引擎（五种条件类型）、心跳探针、Cron 定时任务、自然语言交互、MCP Server、A2UI 聊天界面、多租户/Workspace/RBAC。

**仍在开发**：Modbus RTU/TCP、ONVIF、SNMP 驱动（驱动框架与 plugin-sdk 已就绪）；AI 驱动匹配为实验性。

**技术约束**：Rust 2024 + Axum + SQLx/SQLite 后端，Lit 3 + Vite + TypeScript 前端（Web Components，nanostore 状态）；前端构建需 Node 24。架构遵循 relay 范式（apps/* → crates/* → core 单向依赖，全部 SQL 在 crates/db）。

**术语**：「物」(thing) 是设备/空间/产线/楼宇的统一本体；`devices` 表即 `things` 表；Workspace 是隔离边界；心跳指 AI 巡检而非探活。

## Brand Commitments

- 名称：TinyIoTHub（官方网站 https://tinyiothub.com，仓库 https://github.com/Grong/tinyiothub）。
- 开源 MIT 协议，可私有化部署、可商用——这是持久承诺。
- **中英双语对等（已确认）**：UI 与文档中、英文都是一等公民，所有文案双语维护。
- 口号：「AI 时代的万物互联底座」「万物智能只需三步」。

## Evidence on Hand

- `README.md`：完整特性清单、API 列表、MQTT 主题契约、快速开始。
- `docs/`：技术文档、用户指南（`docs/guide/`）、API 文档（`docs/api/`）、设计规格（`docs/superpowers/specs/`）。
- `examples/`：插件示例与 BACnet 驱动示例。
- `web/DESIGN_SYSTEM.md`：前端自有设计系统规范。
- 真实部署产物：Docker Hub 镜像、官方网站。

**不存在、不得虚构**：客户证言、案例研究、媒体报道、性能基准对比数据、定价信息。

## Product Principles

1. **AI 自治为先**——新功能先问「这能否交给 Agent 闭环完成」，而不是再加一个人工操作界面。
2. **物为本体**——一切接入对象建模为层级化的物，属性/事件/操作 + 知识文档是 AI 理解世界的基础。
3. **轻量是手段**——单进程、零外部依赖、低资源占用，服务于「AI 自治能部署到任何现场」这一目标；不为轻量牺牲自治能力。
4. **双语对等**——中英文案同步维护，不出现单一语言的半成品界面。
5. **开放可私有**——MIT 开源、标准协议（MQTT/MCP/DTDL/WoT）、可私有化部署，不制造供应商锁定。
