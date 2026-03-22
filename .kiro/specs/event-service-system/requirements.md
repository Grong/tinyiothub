# 事件服务系统需求文档

## 介绍

本文档定义了IoT边缘网关系统的完整事件服务架构，包括系统事件、设备事件、事件级别管理、富文本内容支持、事件存储、查询、通知等功能。该系统将替换现有的简单消息系统，提供更强大、更灵活的事件处理能力。

## 术语表

- **Event_System**: 事件系统，负责事件的生成、处理、存储和分发
- **System_Event**: 系统事件，包括用户登录、用户操作、系统异常等
- **Device_Event**: 设备事件，包括设备离线、属性报警、指令执行等
- **Event_Level**: 事件级别，包括故障、错误、警告、消息、调试五个级别
- **Rich_Content**: 富文本内容，支持文字、图片、链接等多媒体信息
- **Event_Handler**: 事件处理器，负责处理特定类型的事件
- **Event_Bus**: 事件总线，负责事件的分发和路由
- **Event_Storage**: 事件存储，负责事件的持久化存储
- **Event_Query**: 事件查询，提供灵活的事件检索功能
- **Event_Notification**: 事件通知，负责事件的实时推送和告警

## 需求

### 需求 1: 事件分类和类型管理

**用户故事:** 作为系统管理员，我希望系统能够清晰地分类和管理不同类型的事件，以便更好地监控和分析系统状态。

#### 验收标准

1. THE Event_System SHALL 支持系统事件分类，包括用户认证、用户操作、系统配置、系统异常四个子类
2. THE Event_System SHALL 支持设备事件分类，包括设备连接、设备属性、设备指令、设备业务四个子类
3. WHEN 创建事件时，THE Event_System SHALL 自动分配正确的事件类型和子类型
4. THE Event_System SHALL 为每个事件类型提供唯一的标识符和描述信息
5. THE Event_System SHALL 支持事件类型的动态扩展和配置
6. THE Event_System SHALL 允许设备驱动自主产生业务事件，如门禁刷卡、传感器读数等

### 需求 2: 事件级别管理

**用户故事:** 作为运维人员，我希望事件按照严重程度进行分级，以便优先处理重要事件。

#### 验收标准

1. THE Event_System SHALL 支持五个事件级别：故障(Critical)、错误(Error)、警告(Warning)、消息(Info)、调试(Debug)
2. WHEN 创建事件时，THE Event_System SHALL 根据事件内容自动分配适当的级别
3. THE Event_System SHALL 为每个级别分配数值权重：故障(5)、错误(4)、警告(3)、消息(2)、调试(1)
4. THE Event_System SHALL 支持基于级别的事件过滤和查询
5. THE Event_System SHALL 为不同级别的事件提供不同的显示样式和颜色标识

### 需求 3: 富文本内容支持

**用户故事:** 作为系统用户，我希望事件内容能够包含丰富的信息格式，包括文字、图片、链接等，以便更好地理解事件详情。

#### 验收标准

1. THE Event_System SHALL 支持结构化的富文本内容格式，包含文本、图片、链接、表格等元素
2. WHEN 存储事件内容时，THE Event_System SHALL 使用JSON格式保存富文本数据结构
3. THE Event_System SHALL 支持图片内容通过文件路径或外部URL引用，不存储 base64
4. THE Event_System SHALL 支持 Markdown 和纯文本 格式的文本内容
5. THE Event_System SHALL 限制单个事件内容大小不超过10MB
6. THE Event_System SHALL 禁止原始 HTML 内容（TextFormat::Html 已移除），防止 XSS 风险

### 需求 4: 系统事件处理

**用户故事:** 作为安全管理员，我希望系统能够记录所有重要的系统操作和异常，以便进行安全审计和问题排查。

#### 验收标准

1. WHEN 用户登录时，THE Event_System SHALL 记录用户认证事件，包含用户ID、IP地址、登录时间、认证结果
2. WHEN 用户执行重要操作时，THE Event_System SHALL 记录用户操作事件，包含操作类型、操作对象、操作参数、操作结果
3. WHEN 系统配置变更时，THE Event_System SHALL 记录配置变更事件，包含变更项、变更前后值、操作用户
4. WHEN 系统发生异常时，THE Event_System SHALL 记录异常事件，包含异常类型、错误信息、堆栈跟踪、影响范围
5. THE Event_System SHALL 为系统事件提供标准化的事件模板和字段定义

### 需求 5: 设备事件处理

**用户故事:** 作为设备管理员，我希望系统能够实时监控设备状态变化和业务事件，以便及时响应设备问题和业务需求。

#### 验收标准

1. WHEN 设备连接状态变化时，THE Event_System SHALL 记录设备连接事件，包含设备ID、连接状态、连接时间、连接方式
2. WHEN 设备属性值变化时，THE Event_System SHALL 记录属性变化事件，包含属性ID、变化前后值、变化时间、变化原因
3. WHEN 设备属性触发报警规则时，THE Event_System SHALL 记录设备属性事件，包含报警规则、触发值、报警级别、报警描述
4. WHEN 设备指令执行时，THE Event_System SHALL 记录指令执行事件，包含指令类型、执行参数、执行结果、执行时间
5. WHEN 设备驱动产生业务事件时，THE Event_System SHALL 记录设备业务事件，包含业务类型、业务数据、描述信息、事件级别
6. THE Event_System SHALL 支持设备事件的批量处理和去重机制

### 需求 6: 事件存储和持久化

**用户故事:** 作为数据管理员，我希望事件数据能够可靠存储并支持高效查询，以便进行历史分析和报表生成。

#### 验收标准

1. THE Event_System SHALL 使用SQLite数据库存储事件数据，支持事务和并发访问
2. THE Event_System SHALL 为事件表创建适当的索引，确保查询性能
3. WHEN 事件数量超过配置限制时，THE Event_System SHALL 自动清理过期事件，保留最近的数据
4. THE Event_System SHALL 支持事件数据的备份和恢复功能
5. THE Event_System SHALL 提供事件数据的导出功能，支持JSON和CSV格式
6. THE Event_System SHALL 确保事件数据的完整性和一致性

### 需求 7: 事件查询和过滤

**用户故事:** 作为系统用户，我希望能够灵活地查询和过滤事件，以便快速找到感兴趣的信息。

#### 验收标准

1. THE Event_System SHALL 支持基于时间范围的事件查询，包含开始时间和结束时间
2. THE Event_System SHALL 支持基于事件级别的过滤查询
3. THE Event_System SHALL 支持基于事件类型和子类型的过滤查询
4. THE Event_System SHALL 支持基于关键词的全文搜索功能
5. THE Event_System SHALL 支持基于设备ID或用户ID的关联查询
6. THE Event_System SHALL 支持分页查询，默认每页20条记录
7. THE Event_System SHALL 支持查询结果的排序，默认按时间倒序排列

### 需求 8: 事件通知和告警

**用户故事:** 作为运维人员，我希望重要事件能够及时通知相关人员，以便快速响应和处理。

#### 验收标准

1. WHEN 故障级别事件发生时，THE Event_System SHALL 立即发送实时通知
2. WHEN 错误级别事件发生时，THE Event_System SHALL 在5分钟内发送通知
3. THE Event_System SHALL 支持多种通知方式，包括WebSocket推送、邮件通知、短信通知
4. THE Event_System SHALL 支持通知规则配置，允许用户自定义通知条件和接收方式
5. THE Event_System SHALL 支持通知的去重和聚合，避免重复通知
6. THE Event_System SHALL 记录通知发送历史和状态

### 需求 9: 事件统计和分析

**用户故事:** 作为系统管理员，我希望能够查看事件的统计信息和趋势分析，以便了解系统运行状况。

#### 验收标准

1. THE Event_System SHALL 提供事件数量的实时统计，按级别和类型分组
2. THE Event_System SHALL 提供事件趋势分析，支持按小时、天、周、月的时间维度
3. THE Event_System SHALL 提供设备事件的TOP排行榜，显示最活跃的设备
4. THE Event_System SHALL 提供用户操作的统计分析，显示操作频率和类型分布
5. THE Event_System SHALL 支持自定义时间范围的统计查询
6. THE Event_System SHALL 提供统计数据的图表展示功能

### 需求 10: API接口和集成

**用户故事:** 作为开发人员，我希望事件系统提供完整的API接口，以便与其他系统集成和扩展。

#### 验收标准

1. THE Event_System SHALL 提供RESTful API接口，支持事件的创建、查询、更新、删除操作
2. THE Event_System SHALL 提供Server-Sent Events (SSE)接口，支持事件的实时推送
3. THE Event_System SHALL 使用统一的API响应格式，包含状态码、消息和数据
4. THE Event_System SHALL 提供API文档和示例代码
5. THE Event_System SHALL 支持API访问的身份认证和权限控制
6. THE Event_System SHALL 提供API调用的限流和监控功能
7. THE Event_System SHALL 提供事件概览API (/api/v1/events/overview)，替代统计API

### 需求 11: 性能和可扩展性

**用户故事:** 作为系统架构师，我希望事件系统具有良好的性能和可扩展性，能够处理大量的事件数据。

#### 验收标准

1. THE Event_System SHALL 支持每秒处理至少1000个事件的吞吐量
2. THE Event_System SHALL 确保事件查询响应时间在100毫秒以内
3. THE Event_System SHALL 支持异步事件处理，避免阻塞主业务流程
4. THE Event_System SHALL 使用连接池管理数据库连接，提高并发性能
5. THE Event_System SHALL 支持事件处理的负载均衡和故障转移
6. THE Event_System SHALL 提供性能监控和调优工具

### 需求 12: 实时事件状态管理

**用户故事:** 作为运维人员，我希望系统能够提供当前系统的实时事件状态快照，让我只关注当前正在发生的问题，而不被历史事件干扰。

#### 验收标准

1. THE Event_System SHALL 维护实时事件状态表，仅保存当前正在发生的事件状态
2. WHEN 设备属性进入报警状态时，THE Event_System SHALL 在实时事件表中创建或更新该属性的当前状态记录
3. WHEN 设备属性恢复正常时，THE Event_System SHALL 从实时事件表中删除该属性的状态记录
4. WHEN 同一设备属性反复报警和恢复时，THE Event_System SHALL 仅在实时事件表中保持最新状态，历史变化记录在事件历史表中
5. THE Event_System SHALL 确保实时事件表中每个设备属性最多只有一条记录，代表当前状态
6. THE Event_System SHALL 提供实时事件状态的查询接口，显示当前所有活跃的问题
7. THE Event_System SHALL 在仪表板中显示实时事件的汇总信息，按级别和类型分组
8. THE Event_System SHALL 支持实时事件的手动确认操作，确认后从实时状态表中移除

### 需求 13: 安全和权限控制

**用户故事:** 作为安全管理员，我希望事件系统具有完善的安全机制，保护敏感事件数据不被未授权访问。

#### 验收标准

1. THE Event_System SHALL 支持基于角色的访问控制(RBAC)，限制用户访问权限
2. THE Event_System SHALL 对敏感事件内容进行加密存储
3. THE Event_System SHALL 记录所有事件访问的审计日志
4. THE Event_System SHALL 支持事件数据的脱敏处理，隐藏敏感信息
5. THE Event_System SHALL 验证事件数据的完整性，防止篡改
6. THE Event_System SHALL 支持安全的事件数据传输，使用HTTPS协议