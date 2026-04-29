# syntax=docker/dockerfile:1

# ============================================
# Stage 0: Chef Planner — 计算依赖配方
# ============================================
FROM rustlang/rust:nightly-bookworm AS planner

RUN cargo install cargo-chef --locked

WORKDIR /build

# 复制整个 workspace 用于分析依赖图
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ============================================
# Stage 1: Frontend Builder — 构建前端静态文件
# ============================================
FROM node:20-alpine AS frontend-builder

WORKDIR /frontend

COPY web/package.json web/pnpm-lock.yaml ./
RUN npm install -g pnpm && pnpm install --frozen-lockfile

COPY web/ ./
RUN pnpm build

# ============================================
# Stage 2: Backend Builder — 编译 Rust 应用
# ============================================
FROM rustlang/rust:nightly-bookworm AS backend-builder

# 安装编译依赖 + mold 链接器
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    perl \
    make \
    mold \
    clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# 复用 planner 阶段的 cargo-chef 二进制
COPY --from=planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef

# Layer 1: 从 recipe 编译所有依赖（Cargo.toml 不变则 layer cache 命中）
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Layer 2: 复制源码并编译应用（仅源码变更时才重新编译）
COPY . .

ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"

# release 构建需要较多内存（建议 Docker 分配 4GB+）
# 本地内存不足时可用 Dockerfile.dev（dev profile）
RUN cargo build --release --bin tinyiothub-cloud && \
    mkdir -p /out && \
    cp /build/target/release/tinyiothub-cloud /out/

# ============================================
# Stage 3: Runtime — 最小化运行时镜像
# ============================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /out/tinyiothub-cloud /app/

COPY --from=backend-builder /build/cloud/migrations /app/migrations
COPY --from=backend-builder /build/cloud/templates /app/templates

COPY --from=frontend-builder /dist/ui /app/wwwroot

COPY app_settings.example.toml /app/app_settings.toml

RUN mkdir -p /app/data /app/logs /app/templates \
    /app/templates/builtin/sensors \
    /app/templates/builtin/cameras \
    /app/templates/builtin/controllers \
    /app/templates/builtin/robots \
    /app/templates/custom \
    /app/templates/schemas && \
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
