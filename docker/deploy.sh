#!/bin/bash
# TinyIoTHub 生产环境部署脚本

set -e

echo "🚀 开始部署 TinyIoTHub..."

# 创建必要的目录
echo "📁 创建数据目录..."
mkdir -p data/certbot data/mosquitto/data data/mosquitto/log logs config templates mosquitto/config nginx/conf.d

# 设置目录权限（使用安全的权限设置）
echo "🔧 设置目录权限..."
chmod -R 755 data logs config templates nginx/conf.d
chmod -R 755 mosquitto/config

# 生成 Mosquitto 密码文件
echo "🔑 生成 Mosquitto 密码文件..."
mosquitto_passwd -c /mosquitto/config/passwd "admin" "TinyIoTHub@2026" || {
    echo "❌ Mosquitto 密码文件生成失败"
    exit 1
}
echo "✅ Mosquitto 密码文件创建成功"

# 设置文件权限为 644（只读）
find data logs config templates -type f -exec chmod 644 {} \; 2>/dev/null || true

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

# 健康检查：等待 API 服务就绪
max_retries=10
retry_count=0
while [ $retry_count -lt $max_retries ]; do
    health=$(docker compose exec -T tinyiothub-api wget -qO- http://localhost:3002/api/health 2>/dev/null)
    if [ "$health" = "{" ] || echo "$health" | grep -q '"status"'; then
        echo "✅ API 服务已就绪"
        break
    fi
    retry_count=$((retry_count + 1))
    echo "⏳ 等待 API 就绪... ($retry_count/$max_retries)"
    sleep 5
done
if [ $retry_count -eq $max_retries ]; then
    echo "❌ API 服务启动超时"
fi

# 检查服务状态
echo "🔍 检查服务状态..."
docker compose ps

# 检查 MQTT 服务状态
if docker compose ps | grep -q "mosquitto.*Up"; then
    echo "✅ MQTT 服务已启动"
else
    echo "⚠️ MQTT 服务启动失败，请检查日志"
fi

# 检查 API 健康状态
echo "🏥 检查 API 健康状态..."
docker compose exec -T tinyiothub-api wget -qO- http://localhost:3002/api/health || echo "⚠️ API 尚未就绪，请稍后检查"

echo "✨ 部署完成！"
echo "访问: https://www.tinyiothub.com"
echo "API: https://api.tinyiothub.com"
echo "MQTT: https://mqtt.tinyiothub.com"
echo "文档: https://docs.tinyiothub.com"
