# ============================================
# Stage 1: Frontend Builder - 构建 Next.js 静态文件
# ============================================
FROM node:20-alpine AS frontend-builder

WORKDIR /frontend

# 复制前端源码
COPY web/package.json web/pnpm-lock.yaml ./
RUN npm install -g pnpm && pnpm install --frozen-lockfile

COPY web/ ./

# 构建静态文件
RUN pnpm build

# ============================================
# Stage 2: Backend Builder - 编译 Rust 应用
# ============================================
FROM rust:1.83-alpine AS backend-builder

# 更新到最新的 nightly 工具链
RUN rustup update nightly && \
    rustup default nightly

# 安装编译依赖
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    make \
    perl

WORKDIR /build

# 第一步：只复制依赖文件，构建依赖缓存
COPY api/Cargo.toml api/Cargo.lock ./api/
COPY api/derive ./api/derive
COPY sdks ./sdks

# 创建虚拟 main.rs 来构建依赖
RUN mkdir -p ./api/src && \
    echo "fn main() {}" > ./api/src/main.rs

WORKDIR /build/api

# 构建依赖（这一层会被缓存）
RUN cargo build --release --bin tinyiothub || true

# 第二步：复制实际源码
WORKDIR /build
COPY api/src ./api/src
COPY api/migrations ./api/migrations
COPY api/templates ./api/templates

# 清理虚拟构建产物，重新编译（只编译项目代码）
WORKDIR /build/api
RUN rm -f ./target/release/deps/tinyiothub* && \
    cargo build --release --bin tinyiothub

# ============================================
# Stage 3: Runtime - 最小化运行时镜像
# ============================================
FROM alpine:latest

# 安装运行时依赖
RUN apk add --no-cache \
    ca-certificates \
    tzdata \
    libgcc

# 设置工作目录
WORKDIR /app

# 从 backend-builder 复制编译产物
COPY --from=backend-builder /build/api/target/release/tinyiothub /app/
COPY --from=backend-builder /build/api/migrations /app/migrations
COPY --from=backend-builder /build/api/templates /app/templates

# 从 frontend-builder 复制静态文件
COPY --from=frontend-builder /frontend/out /app/wwwroot

# 复制配置文件作为默认配置
COPY api/app_settings.example.toml /app/app_settings.toml

# 创建数据目录
RUN mkdir -p /app/data /app/logs /app/templates && \
    chmod -R 777 /app/data /app/logs /app/templates

# 设置环境变量
ENV RUST_LOG=info \
    TZ=Asia/Shanghai \
    TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db \
    JWT_SECRET="" \
    MQTT_USERNAME="" \
    MQTT_PASSWORD=""

# 暴露端口
EXPOSE 3002

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3002/api/health || exit 1

# 创建非root用户
RUN addgroup -g 1001 -S appgroup && adduser -u 1001 -S appuser -G appgroup
USER appuser

# 启动应用
CMD ["/app/tinyiothub"]
