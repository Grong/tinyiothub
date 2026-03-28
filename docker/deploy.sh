#!/bin/bash
# TinyIoTHub 生产环境部署脚本

set -e

echo "🚀 开始部署 TinyIoTHub..."

# 创建必要的目录
echo "📁 创建数据目录..."
mkdir -p data/certbot data/mosquitto/data data/mosquitto/log

# 检查 SSL 证书是否存在，选择合适的 nginx 配置
if [ -d "./nginx/ssl/live/www.tinyiothub.com" ]; then
    echo "🔐 检测到 SSL 证书，使用 HTTPS 配置..."
    cp nginx/conf.d-ssl/*.conf nginx/conf.d/
else
    echo "⚠️ 未检测到 SSL 证书，使用 HTTP 配置..."
    cp nginx/conf.d-http-only/*.conf nginx/conf.d/
fi

# 拉取最新镜像
echo "📦 拉取最新镜像..."
docker compose pull

# 停止旧容器
echo "🛑 停止旧容器..."
docker compose down

# 启动新容器
echo "✅ 启动新容器..."
docker compose up -d

# 等待服务启动
echo "⏳ 等待服务启动..."
sleep 10

# 检查服务状态
echo "🔍 检查服务状态..."
docker compose ps

# 检查 API 健康状态
echo "🏥 检查 API 健康状态..."
docker compose exec -T tinyiothub-api wget -qO- http://localhost:3002/api/health || echo "⚠️ API 尚未就绪，请稍后检查"

echo "✨ 部署完成！"
echo "访问: https://www.tinyiothub.com"
echo "API: https://api.tinyiothub.com"
echo "MQTT: https://mqtt.tinyiothub.com"
echo "文档: https://docs.tinyiothub.com"
