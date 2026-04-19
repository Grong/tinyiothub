# ============================================
# Stage 1: Frontend Builder - 构建前端静态文件
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
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY sdks ./sdks
COPY vendor ./vendor
COPY api/Cargo.toml ./api/
COPY cloud/Cargo.toml ./cloud/
COPY bin/tinyiothub-gateway/Cargo.toml ./bin/tinyiothub-gateway/
COPY bin/tinyiothub-edge/Cargo.toml ./bin/tinyiothub-edge/

# 创建虚拟 src 来构建依赖
RUN mkdir -p ./api/src && echo "fn main() {}" > ./api/src/main.rs && \
    mkdir -p ./cloud/src && echo "fn main() {}" > ./cloud/src/main.rs && \
    mkdir -p ./bin/tinyiothub-gateway/src && echo "fn main() {}" > ./bin/tinyiothub-gateway/src/main.rs && \
    mkdir -p ./bin/tinyiothub-edge/src && echo "fn main() {}" > ./bin/tinyiothub-edge/src/main.rs

# 构建依赖（这一层会被缓存）
RUN cargo build --release --bin tinyiothub --bin tinyiothub-cloud --bin tinyiothub-gateway --bin tinyiothub-edge || true

# 第二步：复制实际源码
COPY api/src ./api/src
COPY api/migrations ./api/migrations
COPY api/templates ./api/templates
COPY cloud/src ./cloud/src
COPY cloud/migrations ./cloud/migrations
COPY cloud/templates ./cloud/templates
COPY bin/tinyiothub-gateway/src ./bin/tinyiothub-gateway/src
COPY bin/tinyiothub-edge/src ./bin/tinyiothub-edge/src

# 清理虚拟构建产物，重新编译（只编译项目代码）
RUN rm -f ./target/release/deps/tinyiothub* ./target/release/deps/tinyiothub_cloud* ./target/release/deps/tinyiothub_gateway* ./target/release/deps/tinyiothub_edge* && \
    cargo build --release --bin tinyiothub --bin tinyiothub-cloud --bin tinyiothub-gateway --bin tinyiothub-edge

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
COPY --from=backend-builder /build/target/release/tinyiothub /app/
COPY --from=backend-builder /build/target/release/tinyiothub-cloud /app/
COPY --from=backend-builder /build/target/release/tinyiothub-gateway /app/
COPY --from=backend-builder /build/target/release/tinyiothub-edge /app/

# 从 backend-builder 复制 migrations/templates
COPY --from=backend-builder /build/api/migrations /app/migrations
COPY --from=backend-builder /build/api/templates /app/templates

# 从前端 builder 复制静态文件
COPY --from=frontend-builder /frontend/dist/ui /app/wwwroot

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

# 默认启动 legacy API binary
CMD ["/app/tinyiothub"]
