# Docker 构建优化指南

## 构建时间问题

Rust 项目的 Docker 构建通常很慢，主要原因：
- 依赖编译时间长（首次构建 8-10 分钟）
- 每次代码修改都重新编译所有依赖

## 优化策略

### 1. 多阶段构建缓存

Dockerfile 已优化为两步构建：

**第一步：构建依赖（可缓存）**
```dockerfile
# 只复制 Cargo.toml 和 Cargo.lock
COPY Cargo.toml Cargo.lock ./

# 创建虚拟 main.rs
RUN mkdir -p ./cloud/src && echo "fn main() {}" > ./cloud/src/main.rs

# 构建依赖（这一层会被 Docker 缓存）
RUN cargo build --release
```

**第二步：构建项目代码**
```dockerfile
# 复制实际源码
COPY cloud/src ./cloud/src

# 只重新编译项目代码（依赖已缓存）
RUN rm -f ./target/release/deps/tinyiothub* && \
    cargo build --release --bin tinyiothub
```

### 2. 构建时间对比

| 构建类型 | 首次构建 | 依赖不变 | 完全重建 |
|---------|---------|---------|---------|
| 未优化 | 8-10分钟 | 8-10分钟 | 8-10分钟 |
| 已优化 | 8-10分钟 | 1-2分钟 | 8-10分钟 |

### 3. 使用优化构建

```bash
# 使用缓存构建（推荐）
.\scripts\docker-build-fast.ps1

# ARM64 构建
.\scripts\docker-build-fast.ps1 -Platform linux/arm64 -Tag arm64

# 强制重新构建（清除缓存）
.\scripts\docker-build-fast.ps1 -NoCache
```

## 最佳实践

### 开发阶段
1. 首次构建后，Docker 会缓存依赖层
2. 修改源码后重新构建，只需 1-2 分钟
3. 只有修改 `Cargo.toml` 时才需要重新构建依赖

### 生产部署
1. 使用 CI/CD 缓存构建层
2. 定期清理旧镜像释放空间
3. 考虑使用预构建的基础镜像

## 进一步优化

### 使用 sccache（可选）

如果需要更快的构建速度，可以使用 sccache：

```dockerfile
# 安装 sccache
RUN cargo install sccache

# 配置环境变量
ENV RUSTC_WRAPPER=sccache
ENV SCCACHE_DIR=/build/.sccache

# 构建时会使用编译缓存
RUN cargo build --release
```

### 使用 cargo-chef（可选）

对于更复杂的项目，可以使用 cargo-chef：

```dockerfile
FROM rust:1.83-alpine AS planner
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.83-alpine AS builder
RUN cargo install cargo-chef
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release
```

## 故障排查

### 缓存未生效
- 检查 `.dockerignore` 是否正确配置
- 确保 `Cargo.toml` 没有修改
- 使用 `docker system df` 检查缓存空间

### 构建失败
- 清除所有缓存：`docker builder prune -a`
- 重新构建：`.\scripts\docker-build-fast.ps1 -NoCache`

### 空间不足
- 清理未使用的镜像：`docker image prune -a`
- 清理构建缓存：`docker builder prune`
