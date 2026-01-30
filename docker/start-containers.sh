#!/bin/sh
set -e

echo "Starting TinyIoTHub containers..."

# 创建自定义网络（如果不存在）
if ! docker network inspect tinyiothub-net >/dev/null 2>&1; then
    echo "Creating network tinyiothub-net..."
    docker network create --subnet=172.30.0.0/16 tinyiothub-net
fi

# 停止并删除旧容器（如果存在）
for container in tinyiothub-api tinyiothub-web tinyiothub-nginx; do
    if docker ps -a --format '{{.Names}}' | grep -q "^${container}$"; then
        echo "Removing old container: ${container}"
        docker stop ${container} >/dev/null 2>&1 || true
        docker rm ${container} >/dev/null 2>&1 || true
    fi
done

# 启动 API 容器
echo "Starting tinyiothub-api..."
docker run -d \
    --name tinyiothub-api \
    --network tinyiothub-net \
    --ip 172.30.0.2 \
    --restart unless-stopped \
    -e RUST_LOG=info \
    -e TZ=Asia/Shanghai \
    -e TINYIOTHUB__DATABASE__URL=/app/data/tinyiothub.db \
    -v /data/tinyiothub/app_settings.toml:/app/app_settings.toml:ro \
    -v /data/tinyiothub/data:/app/data \
    -v /data/tinyiothub/logs:/app/logs \
    --privileged \
    tinyiothub-api:arm64

# 等待 API 启动
echo "Waiting for API to be healthy..."
sleep 5

# 启动 Web 容器
echo "Starting tinyiothub-web..."
docker run -d \
    --name tinyiothub-web \
    --network tinyiothub-net \
    --ip 172.30.0.3 \
    --restart unless-stopped \
    -e NODE_ENV=production \
    -e TZ=Asia/Shanghai \
    -e NEXT_PUBLIC_API_PREFIX=/api/v1 \
    tinyiothub-web:arm64

# 等待 Web 启动
echo "Waiting for Web to be ready..."
sleep 5

# 启动 Nginx 容器
echo "Starting tinyiothub-nginx..."
docker run -d \
    --name tinyiothub-nginx \
    --network tinyiothub-net \
    --ip 172.30.0.4 \
    --restart unless-stopped \
    -p 0.0.0.0:8099:80 \
    -v /data/tinyiothub/nginx/nginx.conf:/etc/nginx/conf.d/default.conf \
    nginx:alpine

echo "All containers started successfully!"
echo "Access the application at: http://<your-ip>:8099"

# 显示容器状态
docker ps --filter "name=tinyiothub-"
