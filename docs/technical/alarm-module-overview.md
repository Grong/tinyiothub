# 报警模块设计概览

## 快速理解

报警模块是一个基于 DDD 的事件驱动系统，负责监控设备状态并在异常时触发报警和通知。

## 核心概念

### 1. 报警实例 (Alarm)
设备或属性触发的具体报警记录，包含：
- 报警级别：Info, Warning, Error, Critical
- 报警状态：Active, Acknowledged, Resolved, Suppressed
- 确认和解决信息

### 2. 报警规则 (AlarmRule)
定义何时触发报警的业务规则，支持：
- 阈值规则：温度 > 80°C
- 范围规则：湿度在 30%-70% 之外
- 变化规则：温度 1 小时内上升 > 10°C
- 持续时间规则：温度持续 5 分钟 > 80°C
- 组合规则：温度 > 80°C AND 湿度 > 90%

### 3. 规则引擎 (RuleEngine)
评估事件是否满足报警条件的核心组件

## 工作流程

```
设备事件 → EventBus → AlarmEventHandler → RuleEngine → AlarmService
                                                            ↓
                                                    创建报警记录
                                                            ↓
                                                    NotificationService
```

## 模块结构

```
src/domain/alarm/
├── entities/          # 实体（Alarm, AlarmRule）
├── value_objects/     # 值对象（AlarmLevel, AlarmType, AlarmCondition）
├── aggregates/        # 聚合根（AlarmAggregate）
├── services/          # 领域服务（AlarmService, RuleEngine）
├── repositories/      # 仓储接口
├── specifications/    # 规约
└── handlers/          # 事件处理器
```

## API 端点

### 报警管理
- `GET /api/v1/alarms` - 查询报警列表
- `GET /api/v1/alarms/:id` - 获取报警详情
- `POST /api/v1/alarms/:id/acknowledge` - 确认报警
- `POST /api/v1/alarms/:id/resolve` - 解决报警
- `POST /api/v1/alarms/batch-acknowledge` - 批量确认
- `GET /api/v1/alarms/statistics` - 报警统计

### 规则管理
- `GET /api/v1/alarm-rules` - 查询规则列表
- `POST /api/v1/alarm-rules` - 创建规则
- `PUT /api/v1/alarm-rules/:id` - 更新规则
- `DELETE /api/v1/alarm-rules/:id` - 删除规则
- `POST /api/v1/alarm-rules/:id/toggle` - 启用/禁用规则

## 数据库表

### device_alarms (报警实例)
- 存储所有报警记录
- 支持确认和解决状态
- 关联设备、属性和规则

### device_alarm_rules (报警规则)
- 存储报警规则配置
- 支持多种规则类型
- 包含通知配置

## 关键特性

1. **实时报警**: 通过事件总线实时处理设备事件
2. **灵活规则**: 支持多种条件类型和组合
3. **报警抑制**: 避免短时间内重复报警
4. **自动解决**: 当条件恢复正常时自动解决报警
5. **多渠道通知**: 集成通知服务，支持 Email, SMS, SSE, Webhook
6. **批量操作**: 支持批量确认和解决
7. **统计分析**: 提供报警趋势和统计数据

## 实现优先级

### P0 (核心功能)
1. 基本的阈值规则
2. 报警创建和查询
3. 报警确认和解决
4. 与事件系统集成

### P1 (重要功能)
1. 多种规则类型（范围、变化、持续时间）
2. 报警抑制
3. 通知集成
4. 批量操作

### P2 (增强功能)
1. 组合规则
2. 自动解决
3. 统计分析
4. 前端界面

## 技术栈

- **后端**: Rust + Axum + SQLx + Tokio
- **数据库**: SQLite
- **前端**: TypeScript + Lit 3 + Vite + nanostore
- **架构**: DDD + Event-Driven + Clean Architecture

## 参考文档

- 详细设计：`docs/alarm-module-design.md`
- 事件架构：`docs/event-handler-architecture.md`
- API 规范：`.kiro/steering/api-standards.md`
