#!/bin/bash
# 初始化 SSL 证书
# 用法: cd docker && bash init-ssl.sh

set -e

EMAIL="${SSL_EMAIL:-admin@tinyiothub.com}"
STAGING_ARG=""
if [ "${SSL_STAGING}" = "1" ]; then
    STAGING_ARG="--staging"
fi

# 创建必要的目录并设置权限
echo "📁 创建数据目录..."
mkdir -p data/certbot data/mosquitto/data data/mosquitto/log logs config templates mosquitto/config nginx/conf.d
chmod -R 777 data logs config templates mosquitto/config nginx/conf.d

# 修复 mosquitto 配置目录权限
echo "🔧 修复 mosquitto 目录权限..."
chown -R root mosquitto/config 2>/dev/null || true

# 检查是否已有证书
if [ -d "./nginx/ssl/live/www.tinyiothub.com" ]; then
    echo "证书已存在，跳过申请。如需重新申请请删除 ./nginx/ssl/live/ 目录"
    exit 0
fi

echo "==> 确保 nginx 使用 HTTP-only 配置..."
cp nginx/conf.d-http-only/*.conf nginx/conf.d/

echo "==> 启动服务..."
docker compose up -d

echo "==> 等待 nginx 就绪..."
sleep 5

echo "==> 申请 www.tinyiothub.com 证书..."
docker compose run --rm --entrypoint "certbot" certbot certonly \
    --webroot -w /var/www/certbot \
    -d www.tinyiothub.com -d tinyiothub.com \
    --email "$EMAIL" \
    --agree-tos --no-eff-email \
    $STAGING_ARG

echo "==> 申请 docs.tinyiothub.com 证书..."
docker compose run --rm --entrypoint "certbot" certbot certonly \
    --webroot -w /var/www/certbot \
    -d docs.tinyiothub.com \
    --email "$EMAIL" \
    --agree-tos --no-eff-email \
    $STAGING_ARG

echo "==> 申请 api.tinyiothub.com 证书..."
docker compose run --rm --entrypoint "certbot" certbot certonly \
    --webroot -w /var/www/certbot \
    -d api.tinyiothub.com \
    --email "$EMAIL" \
    --agree-tos --no-eff-email \
    $STAGING_ARG

echo "==> 申请 mqtt.tinyiothub.com 证书..."
docker compose run --rm --entrypoint "certbot" certbot certonly \
    --webroot -w /var/www/certbot \
    -d mqtt.tinyiothub.com \
    --email "$EMAIL" \
    --agree-tos --no-eff-email \
    $STAGING_ARG

echo "==> 申请 marketplace.tinyiothub.com 证书..."
docker compose run --rm --entrypoint "certbot" certbot certonly \
    --webroot -w /var/www/certbot \
    -d marketplace.tinyiothub.com \
    --email "$EMAIL" \
    --agree-tos --no-eff-email \
    $STAGING_ARG

echo "==> 切换到 HTTPS 配置..."
cp nginx/conf.d-ssl/*.conf nginx/conf.d/

echo "==> 重启 nginx..."
docker compose restart tinyiothub-nginx

echo "==> SSL 证书申请完成！"
