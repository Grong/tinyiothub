# TinyIoTHub 市场数据

这个目录包含 TinyIoTHub 市场的资源元数据。

## 目录结构

```
marketplace/
├── templates/
│   ├── index.json              # 模板列表索引
│   └── [template-files].json  # 具体模板文件
├── drivers/
│   ├── index.json              # 驱动列表索引
│   └── [driver-files]/         # 驱动文件和元数据
└── README.md
```

## 使用方式

### 1. 本地开发测试

将此目录放在项目根目录下，配置文件指向本地路径：

```toml
[marketplace]
enabled = true
source_type = "local"
local_path = "marketplace"
```

### 2. GitHub 托管（推荐）

1. 创建 GitHub 仓库：`tinyiothub/marketplace`
2. 上传此目录内容
3. 配置文件指向 GitHub：

```toml
[marketplace]
enabled = true
source_type = "github"
github_repo = "tinyiothub/marketplace"
github_branch = "main"
```

### 3. 自定义 API 服务器

部署独立的市场服务器，配置 API 地址：

```toml
[marketplace]
enabled = true
source_type = "api"
api_url = "https://marketplace.tinyiothub.com/api/v1"
```

## 数据格式

### 模板索引 (templates/index.json)

```json
{
  "version": "1.0.0",
  "updated_at": "2025-01-29T10:00:00Z",
  "templates": [
    {
      "id": "template-id",
      "name": "模板名称",
      "version": "1.0.0",
      "category": "sensor|camera|controller|robot",
      "protocol": "modbus|onvif|mqtt|...",
      "manufacturer": "厂商名称",
      "description": "模板描述",
      "tags": ["tag1", "tag2"],
      "author": {
        "name": "作者名称",
        "email": "email@example.com"
      },
      "downloads": 1000,
      "rating": 4.5,
      "reviews": 20,
      "license": "MIT",
      "file_url": "模板文件下载地址",
      "checksum": "sha256:...",
      "size": 10240,
      "created_at": "2024-01-01T00:00:00Z",
      "updated_at": "2024-12-31T23:59:59Z"
    }
  ]
}
```

### 驱动索引 (drivers/index.json)

```json
{
  "version": "1.0.0",
  "updated_at": "2025-01-29T10:00:00Z",
  "drivers": [
    {
      "id": "driver-id",
      "name": "驱动名称",
      "version": "1.0.0",
      "protocol": "bacnet|opcua|snmp|...",
      "description": "驱动描述",
      "tags": ["tag1", "tag2"],
      "author": {
        "name": "作者名称",
        "email": "email@example.com"
      },
      "downloads": 500,
      "rating": 4.8,
      "reviews": 15,
      "license": "MIT|Apache-2.0",
      "homepage": "项目主页",
      "documentation": "文档地址",
      "platforms": {
        "windows-x64": {
          "file_url": "下载地址",
          "checksum": "sha256:...",
          "size": 2048000
        },
        "linux-x64": {
          "file_url": "下载地址",
          "checksum": "sha256:...",
          "size": 1800000
        },
        "linux-armv7": {
          "file_url": "下载地址",
          "checksum": "sha256:...",
          "size": 1600000
        }
      },
      "requirements": {
        "min_version": "1.0.0"
      },
      "created_at": "2024-01-01T00:00:00Z",
      "updated_at": "2024-12-31T23:59:59Z"
    }
  ]
}
```

## 添加新资源

### 添加模板

1. 准备模板 JSON 文件
2. 上传到 `templates/` 目录
3. 更新 `templates/index.json`，添加新条目
4. 更新 `updated_at` 时间戳

### 添加驱动

1. 编译各平台的驱动二进制文件
2. 创建 GitHub Release 并上传文件
3. 更新 `drivers/index.json`，添加新条目
4. 填写各平台的下载地址和 checksum
5. 更新 `updated_at` 时间戳

## 版本管理

- 使用语义化版本号：`major.minor.patch`
- 每次更新资源时增加版本号
- 保持向后兼容性

## 安全性

- 所有文件必须提供 SHA256 checksum
- 建议使用 HTTPS 下载链接
- 定期审查和更新资源

## 许可证

市场中的资源可能使用不同的许可证，请查看每个资源的 `license` 字段。
