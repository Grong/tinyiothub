---
layout: home

hero:
  name: "TinyIoTHub"
  text: "物联网边缘网关系统"
  tagline: 基于 Rust 的高性能 IoT 边缘网关，支持多协议（Modbus、ONVIF、SNMP、MQTT）
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
    details: MCP Server 支持 Claude Desktop、Cursor 等 AI 客户端自然语言控制设备
  - title: 规则引擎
    details: 灵活的告警规则配置，支持阈值、条件、通知渠道
  - title: 设备模板
    details: JSON 模板快速创建设备，一键配置
  - title: SaaS 多租户
    details: 支持云端部署，多租户隔离，订阅管理
---
