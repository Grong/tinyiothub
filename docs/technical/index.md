# 技术文档

## 架构

- [事件处理器架构](./event-handler-architecture.md) — DDD 分层架构，事件处理链路

## 告警模块

- [告警模块概览](./alarm-module-overview.md) — 核心概念和工作流
- [告警实现进度](./alarm-implementation-progress.md) — 各阶段完成状态

## 部署

- [Docker 构建优化](./docker-build-optimization.md) — 多架构构建、本地镜像优化

## 配置文件

- [配置系统](./configuration/configuration-system.md) — 配置架构设计
- [配置迁移](./configuration/migration-guide.md) — 配置格式迁移

## 规划中

以下功能正在规划中，详细设计见 `.kiro/specs/`：

- **event-service-system**: 事件驱动架构升级（SSE 推送、富文本、通知渠道）
- **device-template-system**: JSON 模板快速创建设备
- **harmonyos-jwt-openssl**: HarmonyOS SIGSEGV 修复

## 历史文档

已归档的历史分析文档见 `.kiro/archive/`（如需要可恢复）。
