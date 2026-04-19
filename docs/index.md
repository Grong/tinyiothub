---
layout: home

hero:
  name: "TinyIoTHub"
  text: "云端 SaaS 物联网平台"
  tagline: 基于 Rust 的高性能云端 SaaS IoT 平台，支持配置和管理边缘网关设备，兼容多协议（Modbus、ONVIF、SNMP、MQTT）
  actions:
    - theme: brand
      text: 快速开始
      link: /getting-started/
    - theme: alt
      text: API 参考
      link: /api/

features:
  - title: 多协议支持
    details: 内置 Modbus RTU/TCP、ONVIF、SNMP、MQTT 等协议驱动，开箱即用
  - title: 事件驱动架构
    details: 基于 Tokio 异步运行时，高并发低延迟，支持 SSE 实时推送
  - title: AI 集成
    details: MCP Server + A2UI 聊天界面，支持 Claude Desktop、Cursor 自然语言控制设备
  - title: 规则引擎
    details: 灵活的告警规则配置，支持阈值、范围、变化、持续时间、组合五种条件类型
  - title: 定时任务调度
    details: Cron 表达式定时任务，Workspace 隔离，支持执行记录和统计
  - title: 工作空间管理
    details: 按物理环境分组设备，每个 Workspace 绑定独立 AI Agent
  - title: 自愈引擎
    details: system/device/task 三级探针，自动故障检测与恢复
  - title: 设备模板
    details: JSON 模板快速创建设备，一键配置
  - title: SaaS 多租户
    details: 支持云端部署，多租户隔离，订阅管理
  - title: 应用市场
    details: 驱动市场、模板市场，支持第三方扩展
---
