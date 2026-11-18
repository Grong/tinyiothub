# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

技术型买方为主（2026-11-14 定位调整后确认）：

- **硬件厂商 / 设备 ODM**：想给自家设备加 agent 能力，不愿自研 harness。
- **机器人 / 自动化公司**：有设备与 agent 需求，缺物理世界的安全执行层。
- **大模型应用团队**：要做「操作设备」类场景，卡在设备接入与安全边界。
- **能源 / 楼宇 / 制造集团的科技子公司**：创新部门而非 IT 采购部门，用架构与安全语言做判断。
- **运维团队**：把巡检、告警处置、故障恢复交给常驻 Assistant，人只在审批与决策时介入。

使用路径：Web 管理界面、REST API（`/api/v1/`）、MCP 端点（`/mcp`）、自写 `SKILL.md`。

次要场景：私有化部署的工厂/楼宇现场，资源受限的边缘环境。

## Product Purpose

**IoT agent harness**：设备接入是底座，设备之上的 agent 运行环境是产品本体。

核心承诺：**给 AI 一副能安全操作物理世界的手。** Skill 按需加载、待办由模型自维护、执行中可提问、子代理隔离上下文、动作走审批与对账、全过程可审计。用户只需要告诉大脑「注意什么、干什么活」，不需要先声明一套 workflow 才能开始工作。

## Positioning

**harness 层是唯一差异化（2026-11-14 确认，取代「AI 自治是唯一差异化」的旧表述）。**

三层结构，价值重心在中间：

| 层 | 内容 | 竞争状态 |
|---|---|---|
| 应用 | 巡检、故障诊断、心跳监控、用户自写技能 | 场景可复制 |
| **Harness** | Session Loop + Skill/Todo/Ask/Subagent + 安全层 | **差异化所在** |
| 设备 | MQTT/Modbus/ONVIF/SNMP、物本体与层级树 | 底座，必须够用 |

三条通用 agent framework 无法复制的护城河：

1. **Effect / Decision 单赢家 + unknown fence**——软件工具调用失败重试是常态；设备上重试可能是重复关机、重复开阀。动作必须记「意图→派发→确认→未知」四态，未知状态停下来对账，绝不自动重放。
2. **事件唤醒 + 本体决策**——Assistant 不是被 prompt 唤醒，而是被设备事件与定时 tick 唤醒，判断依据是物模型的属性、层级与知识文档。通用 harness 没有世界模型，只能靠用户喂上下文。
3. **回读验证 + 全量审计**——动作下发不算完成，必须回读属性确认真的生效；全程进入安全/审计账本。

**不与之竞争**：通用 agent framework（Claude Code、OpenClaw、LangGraph 等）管比特流，不管物理世界；传统 IoT 平台（ThingsBoard 等）有设备接入，没有 agent 运行时。

**定位锚点（叙述用，非商标口号）**：「Claude Code 给代码写 harness，TinyIoTHub 给设备写 harness。」

## Operating Context

- 单进程私有化部署：二进制 + 内置 SQLite，无外部数据库依赖；Docker 多架构镜像（Docker Hub `grong/tinyiothub`）。
- Harness 事件存储按 workspace 独立：`data/agents/<workspace>/harness/harness.db`。
- 唤醒节拍：`TriggerRouter` 每 30 秒扫描到期 Schedule 与设备事件，每 workspace 至多一个在途 wake，事件按 workspace+trigger 做 3/min 去重。
- 边缘网关（apps/edge）通过 MQTT 上报数据，主题契约见 README「MQTT 主题」与 `docs/device-event-contract.md`。
- 部署目标：Linux、Windows；HarmonyOS 实验性。

## Capabilities and Constraints

**Harness 层（已可用）**
- 统一 Session Loop：`SessionRunner` 是 Assistant Turn 的唯一执行入口，对话、定时巡检、事件处置共用一条循环。
- 触发与执行分离：`TriggerRouter` 只产生 wake，不执行 Turn。
- 主流 harness 五个 primitive：`SKILL.md` 渐进披露、`todo_write`、`ask_user`（1–4 问、可多选、阻塞 turn）、`spawn_subagent`（同步、一层、上下文隔离）；（Plan mode 尚未实现）
- 技能集（`apps/cloud/templates/skills/`，2026-11-14 核对）：**已交付 10 个**——巡检、故障诊断、心跳监控、告警管理、设备管理、驱动管理、作业管理、可视化、workspace、平台概览，`SKILL.md` 形态，无自定义任务 DSL。
- 路线图技能（**规划中，未交付**）：应急预案、节能策略、视频分析。规划中的能力可以对外讲，但必须标注状态，不得用现有能力口吻表述。
- 安全层：policy / trust / approval、Effect/Decision 单赢家、unknown fence 对账、恢复语义。
- 可见性：`session_events` 作为唯一 UI 时间线（session/turn/skill/todo/question/subagent/tool/approval 事件全量），全局 HUD 展示状态、设备、告警、待答、待批、调度。
- 权限三档：`read_only` / `ask`（默认）/ `full_access`；`full_access` 只免除逐次批准，不绕过联锁与 e-stop。
- 零配置可运行：新 workspace 创建后 Assistant 即有内置技能与默认职责。

**设备与本体（已可用）**
- 物本体模型：设备、空间、产线、楼宇统一为层级化「物」，属性/事件/操作 + 知识文档。
- MQTT 驱动；物模板（DTDL/WoT 导入、DTDL 导出）；L0–L3 分级自愈引擎；规则引擎（五种条件类型）；Cron 定时任务；多租户/Workspace/RBAC。
- MCP Server 内嵌，Claude Desktop、Cursor 可直接连接。

**仍在开发**
- Modbus RTU/TCP、ONVIF、SNMP 驱动（驱动框架与 plugin-sdk 已就绪）。
- 从 Workflow-first 架构迁移到统一 Session Loop 的收尾清理（`thing_agent/`、`workflows/`、`host/` 旧路径尚未删净）。
- 旧 `Workflow` 声明式执行已退役，不再作为产品路径；Effect/Decision/恢复内核保留。

**技术约束**
- Rust 2024 + Axum + SQLx/SQLite，Lit 3 + Vite + TypeScript（前端构建需 Node 24）。
- 架构遵循 relay 范式（apps/* → crates/* → core 单向依赖，全部 SQL 在 crates/db）。
- Agent 引擎基于 rig 0.42，业务代码只依赖反腐败层 `crates/agent/src/port`。

**术语**
「物」(thing) 是设备/空间/产线/楼宇的统一本体；`devices` 表即 `things` 表；Workspace 是隔离边界；Assistant 是每 workspace 的常驻身份（产品文案可用「大脑」）；Session 是独立的对话/执行线程（kind = chat/schedule/event）；Skill 是工作手册；Schedule 是周期触发配置。命名规范以 `docs/superpowers/specs/2026-11-07-assistant-harness-design.md` §5.4 为准。

## Brand Commitments

- 名称：TinyIoTHub（官网 https://tinyiothub.com，仓库 https://github.com/Grong/tinyiothub）。
- 开源 MIT 协议，可私有化部署、可商用——持久承诺。
- **中英双语对等（已确认）**：UI 与文档中、英文都是一等公民，所有文案双语维护。
- 对外口号：「让 AI 真正动手」/ "Give your agent hands."
- 定位锚点（仅用于解释，不作商标）：「Claude Code 给代码写 harness，我们给设备写」。
- 双轨表述（供对外沟通）：技术场合用「IoT agent harness」；商务场合用「会让设备自己干活的智能体底座」。同一件事两套说法，不混用。

## Evidence on Hand

- `README.md`：特性清单、三层架构、API 列表、MQTT 主题契约、快速开始。
- `docs/superpowers/specs/2026-11-07-assistant-harness-design.md`：harness 目标模型、五个 primitive、术语规范、迁移批次。
- `docs/designs/agent-harness-core.md`：被取代的 Workflow-first 设计（HISTORICAL），含推翻它的三条失败面，可作为公开复盘素材。
- `docs/`：技术文档、用户指南（`docs/guide/`）、API 文档（`docs/api/`）、设备事件契约。
- `web/DESIGN_SYSTEM.md`：前端自有设计系统规范。
- 真实部署产物：Docker Hub 镜像、官方网站。

**不存在、不得虚构**：客户证言、案例研究、媒体报道、性能基准对比数据、定价信息。

**表述规则**：规划中的能力可以写、可以讲，但必须显式标注「规划中 / 未交付」；不得以现有能力口吻描述。判断标准是时态，不是有没有提到。

## Product Principles

1. **harness 优先**——新能力先问「这是 harness 的职责，还是应用的职责」，harness 保持场景无关；场景差异写进 `SKILL.md`，不写进循环。
2. **物理安全不可协商**——任何统一循环、任何权限模式都不放松 policy / Effect / 对账 / 联锁；`full_access` 不等于免检。
3. **物为本体**——一切接入对象建模为层级化的物，属性/事件/操作 + 知识文档是 agent 理解世界的基础。
4. **轻量是手段**——单进程、零外部依赖、低资源占用，服务于 harness 能部署到任何现场；不为轻量牺牲自治能力。
5. **双语对等**——中英文案同步维护，不出现单一语言的半成品界面。
6. **开放可私有**——MIT 开源、标准协议（MQTT/MCP/DTDL/WoT）、可私有化部署，不制造供应商锁定。
