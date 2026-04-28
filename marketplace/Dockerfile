# ============================================
# TinyIoTHub Marketplace
# ============================================
FROM rustlang/rust:nightly-bookworm AS backend-builder

RUN sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources && \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    perl \
    make \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY . .

RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --bin tinyiothub-marketplace && \
    mkdir -p /out && \
    cp /build/target/release/tinyiothub-marketplace /out/

# ============================================
# Runtime
# ============================================
FROM debian:bookworm-slim

RUN sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources && \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /out/tinyiothub-marketplace /app/
COPY --from=backend-builder /build/marketplace/templates /app/templates

RUN mkdir -p /app/data /app/logs /app/templates && \
    chown -R 1001:1001 /app/data /app/logs /app/templates

ENV RUST_LOG=info \
    TZ=Asia/Shanghai \
    TINYIOTHUB__PROJECT_ROOT=/app \
    TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db

EXPOSE 3003

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3003/ || exit 1

RUN groupadd -g 1001 appgroup && useradd -u 1001 -g appgroup appuser
USER appuser

CMD ["/app/tinyiothub-marketplace"]
