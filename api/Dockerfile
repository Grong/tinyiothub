# ============================================
# Stage 1: Builder - 编译 Rust 应用
# ============================================
FROM rust:1.83-alpine AS builder

# 更新到最新的 nightly 工具链（支持 edition2024）
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

# 复制所有源代码和依赖文件
COPY Cargo.toml Cargo.lock ./
COPY derive ./derive
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates

# 直接编译应用
RUN cargo build --release --bin tinyiothub

# ============================================
# Stage 2: Runtime - 最小化运行时镜像
# ============================================
FROM alpine:latest

# 安装运行时依赖
RUN apk add --no-cache \
    ca-certificates \
    tzdata \
    libgcc

# 创建非 root 用户（可选，串口访问可能需要 root）
RUN addgroup -g 1000 tinyiothub && \
    adduser -D -u 1000 -G tinyiothub tinyiothub

# 设置工作目录
WORKDIR /app

# 从 builder 复制编译产物
COPY --from=builder /build/target/release/tinyiothub /app/
COPY --from=builder /build/migrations /app/migrations
COPY --from=builder /build/templates /app/templates

# 复制配置文件模板和启动脚本
COPY app_settings.example.toml /app/app_settings.example.toml
COPY docker/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# 创建数据目录
RUN mkdir -p /app/data /app/logs && \
    chown -R tinyiothub:tinyiothub /app && \
    chmod -R 755 /app/data /app/logs

# 设置环境变量
ENV RUST_LOG=info \
    TZ=Asia/Shanghai \
    TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db

# 暴露端口（虽然使用 host 模式，但作为文档说明）
EXPOSE 3002

# 设置工作目录为 /app（确保迁移文件路径正确）
WORKDIR /app

# 使用 root 用户运行（串口访问需要）
USER root

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3002/api/health || exit 1

# 使用启动脚本
ENTRYPOINT ["/app/entrypoint.sh"]
