# syntax=docker/dockerfile:1

# ============================================
# TinyIoTHub Marketplace
# ============================================

# ============================================
# Stage 0: Chef Planner — 计算依赖配方
# ============================================
FROM rustlang/rust:nightly-bookworm AS planner

RUN cargo install cargo-chef --locked

WORKDIR /build
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ============================================
# Stage 1: Backend Builder
# ============================================
FROM rustlang/rust:nightly-bookworm AS backend-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    perl \
    make \
    mold \
    clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY --from=planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef

COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"

RUN cargo build --release --bin tinyiothub-marketplace && \
    mkdir -p /out && \
    cp /build/target/release/tinyiothub-marketplace /out/

# ============================================
# Stage 2: Runtime
# ============================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /out/tinyiothub-marketplace /app/
COPY --from=backend-builder /build/marketplace/templates /app/seed/templates
COPY --from=backend-builder /build/marketplace/drivers /app/seed/drivers

RUN mkdir -p /app/data /app/logs /app/seed && \
    chown -R 1001:1001 /app/data /app/logs /app/seed

ENV RUST_LOG=info \
    TZ=Asia/Shanghai \
    TINYIOTHUB__PROJECT_ROOT=/app \
    TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db

EXPOSE 3003

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3003/health || exit 1

RUN groupadd -g 1001 appgroup && useradd -u 1001 -g appgroup appuser
USER appuser

CMD ["/app/tinyiothub-marketplace"]
