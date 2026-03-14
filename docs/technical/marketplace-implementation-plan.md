# 市场功能实施计划（基于现有系统）

## 现状分析

### 已有功能
✅ **模板系统**
- 模板存储：`templates/builtin/` 和 `templates/custom/`
- 模板 API：完整的 CRUD 操作
- 前端界面：模板市场页面（`web/app/components/templates/marketplace/`）
- 模板分类、搜索、过滤功能

✅ **驱动系统**
- 驱动加载：静态驱动 + 动态驱动
- 驱动目录：`drivers/`
- 驱动 API：加载、卸载、列表
- 前端界面：驱动管理页面（`/system/drivers`）

### 缺少功能
❌ 从远程市场下载资源
❌ 资源版本管理
❌ 更新检查和通知
❌ 资源元数据管理
❌ 下载进度显示

## 实施方案（最小化改动）

### 方案：扩展现有系统

**核心思路**：
1. 保持现有的模板和驱动系统不变
2. 添加"市场源"配置，支持从远程获取资源列表
3. 添加下载和安装功能
4. 前端添加"从市场安装"按钮

## 详细实施步骤

### 第一步：配置市场源（1小时）

#### 1.1 更新配置文件
```toml
# app_settings.toml
[marketplace]
enabled = true
# 官方市场 API 地址
api_url = "https://marketplace.tinyiothub.com/api/v1"
# 或使用 GitHub 作为市场源
github_repo = "tinyiothub/marketplace"
github_branch = "main"
# 缓存配置
cache_ttl_hours = 24
# 下载配置
download_timeout_secs = 300
```

#### 1.2 添加配置结构
```rust
// src/infrastructure/config/settings.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub github_repo: Option<String>,
    #[serde(default = "default_github_branch")]
    pub github_branch: String,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_hours: u64,
    #[serde(default = "default_download_timeout")]
    pub download_timeout_secs: u64,
}
```

### 第二步：实现市场数据获取（2-3小时）

#### 2.1 创建市场客户端
```rust
// src/domain/marketplace/client.rs
pub struct MarketplaceClient {
    http_client: reqwest::Client,
    config: MarketplaceConfig,
}

impl MarketplaceClient {
    /// 获取模板列表
    pub async fn fetch_templates(&self) -> Result<Vec<TemplateMetadata>, Error> {
        // 从 GitHub 或 API 获取模板列表
    }
    
    /// 获取驱动列表
    pub async fn fetch_drivers(&self) -> Result<Vec<DriverMetadata>, Error> {
        // 从 GitHub 或 API 获取驱动列表
    }
    
    /// 下载资源文件
    pub async fn download_resource(&self, url: &str, dest: &Path) -> Result<(), Error> {
        // 下载文件并保存
    }
}
```

#### 2.2 创建元数据结构
```rust
// src/domain/marketplace/metadata.rs
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: String,
    pub author: String,
    pub downloads: u64,
    pub rating: f32,
    pub download_url: String,
    pub checksum: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DriverMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub platforms: HashMap<String, PlatformBinary>,
    pub author: String,
    pub downloads: u64,
    pub rating: f32,
}
```

### 第三步：实现下载和安装（3-4小时）

#### 3.1 模板安装器
```rust
// src/domain/marketplace/template_installer.rs
pub struct TemplateInstaller {
    client: Arc<MarketplaceClient>,
    repository: Arc<TemplateRepository>,
}

impl TemplateInstaller {
    /// 从市场安装模板
    pub async fn install_from_marketplace(
        &self,
        template_id: &str,
        version: Option<&str>,
    ) -> Result<DeviceTemplate, Error> {
        // 1. 获取模板元数据
        // 2. 下载模板文件
        // 3. 验证 checksum
        // 4. 保存到 templates/custom/
        // 5. 导入到数据库
    }
}
```

#### 3.2 驱动安装器
```rust
// src/domain/marketplace/driver_installer.rs
pub struct DriverInstaller {
    client: Arc<MarketplaceClient>,
}

impl DriverInstaller {
    /// 从市场安装驱动
    pub async fn install_from_marketplace(
        &self,
        driver_id: &str,
        version: Option<&str>,
    ) -> Result<String, Error> {
        // 1. 获取驱动元数据
        // 2. 选择当前平台的二进制文件
        // 3. 下载驱动文件
        // 4. 验证 checksum
        // 5. 保存到 drivers/ 目录
        // 6. 自动加载驱动
    }
}
```

### 第四步：添加 API 端点（2小时）

#### 4.1 市场 API
```rust
// src/api/marketplace/mod.rs
pub fn create_router() -> Router<AppState> {
    Router::new()
        // 模板市场
        .route("/templates", get(list_marketplace_templates))
        .route("/templates/:id", get(get_marketplace_template))
        .route("/templates/:id/install", post(install_marketplace_template))
        
        // 驱动市场
        .route("/drivers", get(list_marketplace_drivers))
        .route("/drivers/:id", get(get_marketplace_driver))
        .route("/drivers/:id/install", post(install_marketplace_driver))
        
        // 更新检查
        .route("/updates", get(check_updates))
}
```

### 第五步：前端集成（3-4小时）

#### 5.1 扩展模板市场页面
```typescript
// web/app/components/templates/marketplace/template-card.tsx
// 添加"从市场安装"按钮

<Button
  onClick={() => handleInstallFromMarketplace(template.id)}
  loading={installing}
>
  从市场安装
</Button>
```

#### 5.2 扩展驱动管理页面
```typescript
// web/app/(commonLayout)/system/drivers/page.tsx
// 添加"浏览市场"按钮

<Button onClick={() => router.push('/system/drivers/marketplace')}>
  浏览驱动市场
</Button>
```

#### 5.3 创建驱动市场页面
```typescript
// web/app/(commonLayout)/system/drivers/marketplace/page.tsx
// 类似模板市场的界面
```

### 第六步：测试和文档（2小时）

## 时间估算

| 任务 | 预计时间 |
|------|---------|
| 配置市场源 | 1小时 |
| 实现市场数据获取 | 2-3小时 |
| 实现下载和安装 | 3-4小时 |
| 添加 API 端点 | 2小时 |
| 前端集成 | 3-4小时 |
| 测试和文档 | 2小时 |
| **总计** | **13-16小时** |

## 市场数据格式（GitHub 方案）

### 目录结构
```
marketplace/
├── templates/
│   ├── index.json          # 模板列表
│   ├── sensors/
│   │   └── modbus-sensor.json
│   └── cameras/
│       └── onvif-camera.json
├── drivers/
│   ├── index.json          # 驱动列表
│   ├── bacnet/
│   │   ├── metadata.json
│   │   └── releases/
│   │       ├── 2.1.0-win-x64.dll
│   │       └── 2.1.0-linux-x64.so
│   └── opcua/
│       └── ...
└── README.md
```

### index.json 示例
```json
{
  "version": "1.0.0",
  "updated_at": "2025-01-29T10:00:00Z",
  "templates": [
    {
      "id": "modbus-sensor",
      "name": "Modbus RTU 传感器",
      "version": "1.2.0",
      "category": "sensor",
      "file": "sensors/modbus-sensor.json",
      "downloads": 1523,
      "rating": 4.8
    }
  ]
}
```

## 快速开始（最小实现）

如果你想快速看到效果，可以先实现：

### Day 1: 基础框架
1. ✅ 添加市场配置
2. ✅ 创建 GitHub 仓库存放市场数据
3. ✅ 实现从 GitHub 获取资源列表

### Day 2: 下载和安装
1. ✅ 实现模板下载和安装
2. ✅ 实现驱动下载和安装
3. ✅ 添加 API 端点

### Day 3: 前端集成
1. ✅ 模板市场添加安装按钮
2. ✅ 驱动管理添加市场入口
3. ✅ 测试完整流程

## 下一步

请告诉我：
1. 你想使用 GitHub 还是独立的 API 服务器作为市场源？
2. 我现在开始实现吗？从哪个部分开始？
3. 你有准备好的市场数据吗？还是需要我帮你创建示例数据？
