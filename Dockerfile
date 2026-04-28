# ============================================
# Stage 1: Frontend Builder - 构建前端静态文件
# ============================================
FROM node:20-alpine AS frontend-builder

WORKDIR /frontend

COPY web/package.json web/pnpm-lock.yaml ./
RUN npm install -g pnpm && pnpm install --frozen-lockfile

COPY web/ ./
RUN pnpm build

# ============================================
# Stage 2: Backend Builder - 编译 Rust 应用
# ============================================
# 使用本地缓存的 nightly 镜像，无需下载工具链
FROM rustlang/rust:nightly-bookworm AS backend-builder

# 安装编译依赖（Debian，使用国内镜像）
RUN sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources && \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    perl \
    make \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# 复制整个 workspace（.dockerignore 已排除 target/node_modules/docs）
COPY . .

# 使用 BuildKit cache 挂载加速后续构建
# 注意：release 构建需要较多内存（建议 Docker 分配 4GB+）
# 本地内存不足时可用 Dockerfile.dev（dev profile）
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --bin tinyiothub-cloud && \
    mkdir -p /out && \
    cp /build/target/release/tinyiothub-cloud /out/

# ============================================
# Stage 3: Runtime - 最小化运行时镜像
# ============================================
FROM debian:bookworm-slim

RUN sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources && \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /out/tinyiothub-cloud /app/

COPY --from=backend-builder /build/cloud/migrations /app/migrations
COPY --from=backend-builder /build/cloud/templates /app/templates

COPY --from=frontend-builder /dist/ui /app/wwwroot

COPY app_settings.example.toml /app/app_settings.toml

RUN mkdir -p /app/data /app/logs /app/templates && \
    chown -R 1001:1001 /app/data /app/logs /app/templates

ENV RUST_LOG=info \
    TZ=Asia/Shanghai \
    TINYIOTHUB__PROJECT_ROOT=/app \
    TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db

EXPOSE 3002

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3002/api/health || exit 1

RUN groupadd -g 1001 appgroup && useradd -u 1001 -g appgroup appuser
USER appuser

CMD ["/app/tinyiothub-cloud"]
