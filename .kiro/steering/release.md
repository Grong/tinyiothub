---
inclusion: manual
description: 发布新版本的完整流程（版本号、CHANGELOG、打 tag、部署）
---

# 发布版本技能

## 发布流程

当用户要求发布新版本时，按以下步骤执行：

### 1. 确认版本号

- 遵循语义化版本：`vMAJOR.MINOR.PATCH`
- 询问用户本次发布的版本号，或根据变更类型建议：
  - Bug 修复 → PATCH（如 v1.0.1）
  - 新功能 → MINOR（如 v1.1.0）
  - 不兼容变更 → MAJOR（如 v2.0.0）

### 2. 更新版本号

更新以下文件中的版本号：
- `api/Cargo.toml` 中的 `version` 字段
- `readme.md` 中的版本标记

### 3. 更新 CHANGELOG.md

- 将 `[Unreleased]` 下的内容移到新版本标题下
- 格式：`## [x.y.z] - YYYY-MM-DD - 简短描述`
- 按 Added / Changed / Fixed / Removed 分类
- 保留空的 `[Unreleased]` 段落

### 4. 代码检查

```bash
cd api
cargo fmt --check
cargo clippy -- -D warnings --allow dead_code --allow unused_imports --allow unused_variables --allow unused_mut --allow non_snake_case 2>&1 | tail -10
cargo check 2>&1 | tail -10
```

### 5. 前端构建验证

```bash
cd web
pnpm build
```

### 6. Docker 构建验证

```bash
docker build -t tinyiothub:test -f Dockerfile . --quiet
```

如果构建失败，先修复问题再继续发布。

### 7. 提交并打 Tag

直接执行（不要让用户手动操作）：

```bash
git add -A
git commit -m "release: vx.y.z"
git tag vx.y.z
git push origin <当前分支> --tags
```

推送 tag 后 GitHub Actions 会自动：
- 构建 linux/amd64 + linux/arm64 多架构 Docker 镜像
- 推送到 Docker Hub: `chenguorongz/tinyiothub`

### 8. 部署

告诉用户在目标服务器上运行：

```bash
docker pull chenguorongz/tinyiothub:latest
docker-compose up -d
```

鸿蒙设备部署：

```bash
.\scripts\deploy-to-ohos.ps1
```

## 项目信息

- 仓库地址：https://github.com/Grong/tinyiothub.git
- Docker Hub：`chenguorongz/tinyiothub`
- 域名：tinyiothub.com
- 分支策略：
  - `master` — 边缘网关版本（单机部署）
  - `saas` — SaaS 平台版本（云端部署）
- CI/CD 配置：`.github/workflows/ci.yml`（push/PR 触发检查）、`.github/workflows/release.yml`（tag 触发构建推送）
- 只有推送 `v*` 格式的 tag 才会触发 Docker 镜像构建和推送
- Docker 镜像 tag 规则：
  - `v1.2.3` tag → `1.2.3`、`1.2`、`latest`
