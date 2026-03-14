# 模板市场和驱动市场设计方案

## 1. 概述

### 1.1 目标
- 提供统一的市场平台，用户可以浏览、下载和安装设备模板和驱动
- 支持版本管理和更新通知
- 提供评分、评论和使用统计
- 支持离线包和在线安装两种方式

### 1.2 架构组件
```
┌─────────────────────────────────────────────────────────────┐
│                      TinyIoTHub 客户端                        │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  模板市场    │  │  驱动市场    │  │  本地管理    │      │
│  │  浏览/搜索   │  │  浏览/搜索   │  │  已安装列表  │      │
│  │  下载/安装   │  │  下载/安装   │  │  更新检查    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
                            ↕ HTTP/HTTPS
┌─────────────────────────────────────────────────────────────┐
│                    市场服务器 (可选)                          │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  资源存储    │  │  元数据管理  │  │  统计分析    │      │
│  │  CDN/OSS     │  │  版本控制    │  │  下载统计    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

## 2. 数据模型

### 2.1 市场资源元数据

#### 模板元数据 (Template Metadata)
```json
{
  "id": "template-modbus-rtu-sensor",
  "name": "Modbus RTU 传感器模板",
  "version": "1.2.0",
  "type": "template",
  "category": "sensor",
  "protocol": "modbus-rtu",
  "author": {
    "name": "TinyIoTHub Team",
    "email": "support@tinyiothub.com",
    "organization": "TinyIoTHub"
  },
  "description": "通用 Modbus RTU 传感器设备模板，支持温度、湿度、压力等常见传感器",
  "tags": ["modbus", "sensor", "rtu", "temperature"],
  "icon": "https://cdn.tinyiothub.com/templates/modbus-sensor.png",
  "screenshots": [
    "https://cdn.tinyiothub.com/templates/modbus-sensor-1.png"
  ],
  "downloads": 1523,
  "rating": 4.8,
  "reviews": 45,
  "license": "MIT",
  "homepage": "https://github.com/tinyiothub/templates",
  "documentation": "https://docs.tinyiothub.com/templates/modbus-sensor",
  "created_at": "2024-01-15T10:00:00Z",
  "updated_at": "2024-12-20T15:30:00Z",
  "download_url": "https://cdn.tinyiothub.com/templates/modbus-rtu-sensor-1.2.0.json",
  "checksum": "sha256:abc123...",
  "size": 15360,
  "requirements": {
    "min_version": "1.0.0",
    "drivers": ["ModbusDriver"]
  }
}
```

#### 驱动元数据 (Driver Metadata)
```json
{
  "id": "driver-bacnet",
  "name": "BACnet 驱动",
  "version": "2.1.0",
  "type": "driver",
  "category": "protocol",
  "protocol": "bacnet",
  "author": {
    "name": "TinyIoTHub Team",
    "email": "support@tinyiothub.com"
  },
  "description": "BACnet 协议驱动，支持 BACnet/IP 和 BACnet MSTP",
  "tags": ["bacnet", "building-automation", "hvac"],
  "icon": "https://cdn.tinyiothub.com/drivers/bacnet.png",
  "downloads": 856,
  "rating": 4.6,
  "reviews": 23,
  "license": "Apache-2.0",
  "homepage": "https://github.com/tinyiothub/drivers/bacnet",
  "documentation": "https://docs.tinyiothub.com/drivers/bacnet",
  "created_at": "2024-03-10T08:00:00Z",
  "updated_at": "2025-01-15T12:00:00Z",
  "platforms": {
    "windows": {
      "x86_64": {
        "download_url": "https://cdn.tinyiothub.com/drivers/bacnet-2.1.0-win-x64.dll",
        "checksum": "sha256:def456...",
        "size": 2457600
      }
    },
    "linux": {
      "x86_64": {
        "download_url": "https://cdn.tinyiothub.com/drivers/bacnet-2.1.0-linux-x64.so",
        "checksum": "sha256:ghi789...",
        "size": 2103296
      },
      "armv7": {
        "download_url": "https://cdn.tinyiothub.com/drivers/bacnet-2.1.0-linux-armv7.so",
        "checksum": "sha256:jkl012...",
        "size": 1945600
      }
    }
  },
  "requirements": {
    "min_version": "1.0.0"
  }
}
```

### 2.2 本地数据库表

#### 已安装资源表 (installed_resources)
```sql
CREATE TABLE installed_resources (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    type TEXT NOT NULL, -- 'template' or 'driver'
    installed_at TEXT NOT NULL,
    installed_from TEXT, -- 'marketplace' or 'local'
    file_path TEXT,
    metadata TEXT, -- JSON
    auto_update BOOLEAN DEFAULT 0,
    UNIQUE(resource_id, type)
);
```

#### 市场缓存表 (marketplace_cache)
```sql
CREATE TABLE marketplace_cache (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    metadata TEXT NOT NULL, -- JSON
    cached_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
```

## 3. API 设计

### 3.1 后端 API

#### 市场资源 API
```rust
// 获取市场资源列表
GET /api/v1/marketplace/{type}/list
Query: ?category=sensor&page=1&page_size=20&sort=downloads

// 搜索市场资源
GET /api/v1/marketplace/{type}/search
Query: ?q=modbus&category=sensor

// 获取资源详情
GET /api/v1/marketplace/{type}/{id}

// 下载资源
POST /api/v1/marketplace/{type}/{id}/download
Body: { "version": "1.2.0" }

// 安装资源
POST /api/v1/marketplace/{type}/{id}/install
Body: { "version": "1.2.0", "auto_update": true }

// 卸载资源
DELETE /api/v1/marketplace/{type}/{id}/uninstall

// 检查更新
GET /api/v1/marketplace/updates
Response: [{ "id": "...", "current": "1.0.0", "latest": "1.2.0" }]

// 获取已安装资源
GET /api/v1/marketplace/installed
Query: ?type=template
```

### 3.2 市场服务器 API (可选)

如果部署独立的市场服务器：

```
GET  /api/v1/resources/{type}           # 列表
GET  /api/v1/resources/{type}/{id}      # 详情
GET  /api/v1/resources/{type}/{id}/versions  # 版本列表
POST /api/v1/resources/{type}/{id}/download  # 下载统计
GET  /api/v1/resources/search           # 搜索
GET  /api/v1/categories                 # 分类列表
```

## 4. 实现方案

### 4.1 阶段一：基础功能（MVP）

#### 后端实现
1. **市场配置**
   - 配置文件中添加市场服务器地址
   - 支持多个市场源（官方 + 第三方）

2. **资源管理模块**
   ```rust
   // src/domain/marketplace/mod.rs
   pub mod resource;
   pub mod installer;
   pub mod updater;
   
   pub struct MarketplaceManager {
       config: MarketplaceConfig,
       http_client: reqwest::Client,
       db: Arc<DataContext>,
   }
   ```

3. **下载和安装**
   - 下载到临时目录
   - 验证 checksum
   - 安装到指定目录
   - 更新数据库记录

4. **API 端点**
   ```rust
   // src/api/marketplace/mod.rs
   pub fn create_router() -> Router<AppState> {
       Router::new()
           .route("/templates", get(list_templates))
           .route("/templates/:id", get(get_template_detail))
           .route("/templates/:id/install", post(install_template))
           .route("/drivers", get(list_drivers))
           .route("/drivers/:id", get(get_driver_detail))
           .route("/drivers/:id/install", post(install_driver))
           .route("/installed", get(list_installed))
           .route("/updates", get(check_updates))
   }
   ```

#### 前端实现
1. **市场页面**
   ```
   web/app/(commonLayout)/marketplace/
   ├── templates/
   │   ├── page.tsx          # 模板市场列表
   │   └── [id]/
   │       └── page.tsx      # 模板详情
   ├── drivers/
   │   ├── page.tsx          # 驱动市场列表
   │   └── [id]/
   │       └── page.tsx      # 驱动详情
   └── installed/
       └── page.tsx          # 已安装资源
   ```

2. **组件设计**
   - `MarketplaceCard` - 资源卡片
   - `ResourceDetail` - 资源详情
   - `InstallButton` - 安装按钮（带进度）
   - `UpdateBadge` - 更新提示徽章

3. **Service 层**
   ```typescript
   // web/service/marketplace.ts
   export const marketplaceApi = {
     listTemplates: (params) => apiGet('marketplace/templates', params),
     getTemplateDetail: (id) => apiGet(`marketplace/templates/${id}`),
     installTemplate: (id, version) => apiPost(`marketplace/templates/${id}/install`, { version }),
     listDrivers: (params) => apiGet('marketplace/drivers', params),
     installDriver: (id, version) => apiPost(`marketplace/drivers/${id}/install`, { version }),
     checkUpdates: () => apiGet('marketplace/updates'),
   }
   ```

### 4.2 阶段二：增强功能

1. **离线包支持**
   - 导出已安装资源为离线包
   - 从离线包导入资源

2. **版本管理**
   - 支持安装特定版本
   - 版本回滚功能
   - 版本对比

3. **依赖管理**
   - 自动检测依赖
   - 批量安装依赖

4. **更新通知**
   - 后台定期检查更新
   - 桌面通知
   - 自动更新选项

### 4.3 阶段三：高级功能

1. **社区功能**
   - 评分和评论
   - 使用统计
   - 推荐算法

2. **开发者功能**
   - 资源上传
   - 版本发布
   - 统计分析

3. **企业功能**
   - 私有市场
   - 访问控制
   - 审核流程

## 5. 配置文件

### 5.1 app_settings.toml 扩展
```toml
[marketplace]
# 市场配置
enabled = true
# 官方市场地址
official_url = "https://marketplace.tinyiothub.com"
# 第三方市场地址（可选）
custom_sources = [
    "https://custom-market.example.com"
]
# 缓存配置
cache_ttl_hours = 24
# 下载配置
download_dir = "downloads"
download_timeout_secs = 300
# 自动更新
auto_check_updates = true
check_interval_hours = 24
```

## 6. 目录结构

```
tinyiothub/
├── templates/              # 本地模板目录
│   ├── official/          # 官方模板
│   └── custom/            # 自定义模板
├── drivers/               # 本地驱动目录
│   ├── official/          # 官方驱动
│   └── custom/            # 自定义驱动
├── downloads/             # 下载临时目录
│   ├── templates/
│   └── drivers/
└── marketplace/           # 市场缓存
    └── cache/
```

## 7. 安全考虑

1. **文件验证**
   - SHA256 checksum 验证
   - 数字签名验证（可选）

2. **沙箱执行**
   - 驱动加载前的安全检查
   - 资源隔离

3. **权限控制**
   - 安装需要管理员权限
   - API 访问控制

4. **网络安全**
   - HTTPS 强制
   - 证书验证
   - 超时控制

## 8. 实施计划

### 第一周：基础架构
- [ ] 数据模型设计和数据库迁移
- [ ] 市场配置和 HTTP 客户端
- [ ] 基础 API 端点

### 第二周：下载和安装
- [ ] 资源下载功能
- [ ] Checksum 验证
- [ ] 模板安装逻辑
- [ ] 驱动安装逻辑

### 第三周：前端界面
- [ ] 市场列表页面
- [ ] 资源详情页面
- [ ] 安装进度显示
- [ ] 已安装资源管理

### 第四周：测试和优化
- [ ] 单元测试
- [ ] 集成测试
- [ ] 性能优化
- [ ] 文档编写

## 9. 测试策略

1. **单元测试**
   - 下载功能测试
   - Checksum 验证测试
   - 安装逻辑测试

2. **集成测试**
   - 完整安装流程测试
   - 更新检查测试
   - 卸载测试

3. **端到端测试**
   - 用户操作流程测试
   - 错误处理测试

## 10. 监控和日志

1. **操作日志**
   - 下载记录
   - 安装记录
   - 更新记录

2. **错误日志**
   - 下载失败
   - 安装失败
   - 验证失败

3. **统计信息**
   - 下载次数
   - 安装成功率
   - 平均下载时间

## 11. 未来扩展

1. **插件系统**
   - 支持更多类型的扩展
   - 插件市场

2. **AI 推荐**
   - 基于使用习惯推荐
   - 智能搜索

3. **协作功能**
   - 团队共享
   - 资源同步

4. **多语言支持**
   - 国际化市场
   - 本地化内容
