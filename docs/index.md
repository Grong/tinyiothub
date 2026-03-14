---
layout: home

hero:
  name: TinyIoTHub
  text: 专业物联网边缘网关系统
  tagline: 基于 Rust 的高性能物联网平台，专为鸿蒙系统优化
  actions:
    - theme: brand
      text: 快速开始 →
      link: /getting-started/
    - theme: alt
      text: 查看演示
      link: /guide/
  image:
    src: /logo.svg
    alt: TinyIoTHub

features:
  - title: 🚀 高性能异步架构
    details: 基于 Tokio 的异步运行时，支持高并发连接，内存占用低至 80MB
  - title: 🔌 多协议支持
    details: 支持 Modbus RTU/TCP、ONVIF、SNMP、Ping 等主流工业协议
  - title: 🌐 现代化 REST API
    details: 基于 Axum 框架，提供统一的 API 响应格式，易于集成
  - title: 📱 MQTT 消息推送
    details: 支持主备双通道，实时推送设备数据和告警信息
  - title: 🔐 安全可靠
    details: JWT 身份认证，配置验证，权限控制
  - title: 🎨 现代化前端
    details: Next.js + TypeScript + TailwindCSS，响应式设计

---

## 🏗️ 系统架构

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Next.js UI    │    │   REST API      │    │   MQTT Client   │
│   (web/)        │    │   (api/)        │    │   (rumqttc)     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌────────────────────────────┐
                    │     Application Layer      │
                    │  数据服务 / 消息服务 / 调度 │
                    └────────────────────────────┘
                                 │
                    ┌────────────────────────────┐
                    │       Domain Layer         │
                    │  设备域 / 告警域 / 事件域   │
                    └────────────────────────────┘
                                 │
                    ┌────────────────────────────┐
                    │   Infrastructure Layer     │
                    │  配置系统 / 硬件抽象 / 存储  │
                    └────────────────────────────┘
```

## 📦 技术栈

| 层级 | 技术 |
|------|------|
| 后端 | Rust 2021 + Axum + Tokio |
| 前端 | Next.js 14 + TypeScript + TailwindCSS |
| 数据库 | SQLite + SQLx |
| 通信 | MQTT, HTTP, Modbus, ONVIF, SNMP |

## 🚀 快速开始

```bash
# 克隆项目
git clone https://github.com/tinyiothub/tinyiothub.git
cd tinyiothub

# 启动后端
cd api
cargo run

# 启动前端
cd web
pnpm install
pnpm dev
```

访问 http://localhost:3001 开始使用！
