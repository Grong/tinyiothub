# 定时任务 API

## 概述

定时任务 API 提供系统定时任务的创建、修改、删除和手动触发功能。支持 HTTP 请求、脚本执行、设备命令和 SQL 查询等多种任务类型。

## 接口列表

### 获取任务列表

```
GET /api/v1/jobs
```

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 否 | 按名称模糊筛选 |
| job_type | string | 否 | 任务类型：http、script、device_command、sql |
| enabled | boolean | 否 | 是否启用 |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
[
  {
    "id": "job_001",
    "name": "每小时数据同步",
    "description": "将设备数据同步到云端",
    "job_type": "http",
    "cron_expression": "0 * * * *",
    "config": "{\"url\":\"/api/sync\",\"method\":\"POST\"}",
    "timeout_seconds": 60,
    "retry_count": 3,
    "enabled": true,
    "is_running": false,
    "last_run_at": "2024-01-07 15:00:00",
    "last_run_status": "success",
    "run_count": 168,
    "success_count": 165,
    "fail_count": 3,
    "created_at": "2024-01-01 10:00:00",
    "updated_at": "2024-01-07 15:00:00"
  }
]
```

---

### 获取任务详情

```
GET /api/v1/jobs/{id}
```

---

### 创建任务

```
POST /api/v1/jobs
```

**请求体：**

```json
{
  "name": "每天备份数据库",
  "description": "每天凌晨2点备份数据库",
  "job_type": "script",
  "cron_expression": "0 2 * * *",
  "config": "{\"script\":\"backup_db.sh\",\"interpreter\":\"bash\",\"working_dir\":\"/opt/scripts\"}",
  "timeout_seconds": 3600,
  "retry_count": 2,
  "retry_delay_seconds": 60,
  "enabled": true
}
```

**任务类型说明：**

| 类型 | 说明 | 配置字段 |
|------|------|----------|
| http | HTTP 请求任务 | url、method、headers、body |
| script | 脚本执行任务 | script、interpreter、working_dir |
| device_command | 设备命令任务 | device_id、command |
| sql | SQL 查询任务 | sql |

**HTTP 任务配置示例：**

```json
{
  "url": "http://example.com/api/sync",
  "method": "POST",
  "headers": {
    "Authorization": "Bearer token"
  },
  "body": {
    "key": "value"
  }
}
```

**脚本任务配置示例：**

```json
{
  "script": "python /opt/scripts/collect_data.py",
  "interpreter": "bash",
  "working_dir": "/opt/scripts"
}
```

---

### 更新任务

```
PUT /api/v1/jobs/{id}
```

**请求体（支持部分更新）：**

```json
{
  "name": "每天备份数据库（已修改）",
  "cron_expression": "0 3 * * *",
  "enabled": false
}
```

---

### 删除任务

```
DELETE /api/v1/jobs/{id}
```

**响应：** `204 No Content`

---

### 启用任务

```
POST /api/v1/jobs/{id}/enable
```

---

### 禁用任务

```
POST /api/v1/jobs/{id}/disable
```

---

### 手动执行任务

```
POST /api/v1/jobs/{id}/run
```

手动触发任务立即执行。

**响应示例：**

```json
{
  "id": "exec_001",
  "job_id": "job_001",
  "triggered_by": "manual",
  "triggered_by_user": "admin",
  "status": "running",
  "started_at": "2024-01-07 16:00:00",
  "finished_at": null,
  "result": null,
  "error": null
}
```

---

### 获取任务执行记录

```
GET /api/v1/jobs/{id}/executions
```

获取指定任务的执行历史。

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
[
  {
    "id": "exec_001",
    "job_id": "job_001",
    "triggered_by": "schedule",
    "triggered_by_user": null,
    "status": "success",
    "started_at": "2024-01-07 15:00:00",
    "finished_at": "2024-01-07 15:00:45",
    "result": "{\"synced\": 125}",
    "error": null,
    "duration_ms": 45000
  },
  {
    "id": "exec_002",
    "job_id": "job_001",
    "triggered_by": "manual",
    "triggered_by_user": "admin",
    "status": "failed",
    "started_at": "2024-01-07 14:00:00",
    "finished_at": "2024-01-07 14:01:00",
    "result": null,
    "error": "Connection timeout",
    "duration_ms": 60000
  }
]
```

---

### 获取全部执行记录

```
GET /api/v1/executions
```

获取所有任务的执行记录。

---

### 获取任务统计

```
GET /api/v1/jobs/statistics
```

**响应示例：**

```json
{
  "total_jobs": 10,
  "enabled_jobs": 8,
  "disabled_jobs": 2,
  "running_jobs": 1,
  "total_executions": 1680,
  "successful_executions": 1650,
  "failed_executions": 30,
  "success_rate": 0.982
}
```

## 任务数据结构

### Job

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 任务 ID |
| name | string | 任务名称 |
| description | string? | 描述 |
| job_type | string | 任务类型 |
| cron_expression | string | Cron 表达式 |
| config | string | 任务配置（JSON） |
| timeout_seconds | number | 超时时间（秒） |
| retry_count | number | 重试次数 |
| retry_delay_seconds | number | 重试间隔（秒） |
| enabled | boolean | 是否启用 |
| is_running | boolean | 是否正在运行 |
| last_run_at | string? | 最后执行时间 |
| last_run_status | string? | 最后执行状态 |
| run_count | number | 总执行次数 |
| success_count | number | 成功次数 |
| fail_count | number | 失败次数 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

### JobExecution

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 执行记录 ID |
| job_id | string | 任务 ID |
| triggered_by | string | 触发方式：schedule、manual |
| triggered_by_user | string? | 触发用户 |
| status | string | 执行状态：running、success、failed |
| started_at | string | 开始时间 |
| finished_at | string? | 结束时间 |
| result | string? | 执行结果（JSON） |
| error | string? | 错误信息 |
| duration_ms | number? | 执行时长（毫秒） |

## Cron 表达式说明

| 表达式 | 说明 |
|--------|------|
| `0 * * * *` | 每小时整点 |
| `0 0 * * *` | 每天午夜 |
| `0 0 * * 0` | 每周日午夜 |
| `0 0 1 * *` | 每月第一天的午夜 |
| `*/5 * * * *` | 每 5 分钟 |
| `0 8-18 * * *` | 每天 8 点到 18 点每小时 |

## 使用场景

### 1. 创建定时数据采集任务

```json
POST /api/v1/jobs
{
  "name": "每小时数据采集",
  "description": "从设备采集最新数据并上传到云端",
  "job_type": "http",
  "cron_expression": "0 * * * *",
  "config": "{\"url\":\"/api/v1/data/collect\",\"method\":\"POST\"}",
  "timeout_seconds": 60,
  "retry_count": 2,
  "enabled": true
}
```

### 2. 执行 Python 脚本任务

```json
POST /api/v1/jobs
{
  "name": "生成日报",
  "job_type": "script",
  "cron_expression": "0 9 * * *",
  "config": "{\"script\":\"python generate_report.py\",\"interpreter\":\"powershell\"}",
  "timeout_seconds": 300,
  "enabled": true
}
```

### 3. 手动触发并跟踪执行结果

```javascript
// 手动触发任务
const exec = await fetch('/api/v1/jobs/job_001/run', { method: 'POST' });

// 获取执行记录
const executions = await fetch('/api/v1/jobs/job_001/executions');
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 204 | 删除成功 |
| 400 | 无效的 Cron 表达式 |
| 404 | 任务不存在 |
| 409 | 任务正在运行中（冲突） |
| 500 | 服务器内部错误 |
