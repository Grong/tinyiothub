#!/bin/bash
# SSL 证书续期脚本
# 用法:
#   cd deploy/docker && bash renew-ssl.sh          # 手动续期 + 重载 nginx
#   或添加到 crontab 实现自动重载:
#   0 3 * * * cd /path/to/deploy/docker && bash renew-ssl.sh >> /var/log/certbot-renew.log 2>&1

set -e

echo "[$(date -Iseconds)] 检查证书续期..."

# 强制续期检查 (certbot 只在证书距离过期 ≤30 天时才真正续期)
docker compose exec -T certbot certbot renew --quiet

# 重载 nginx 以加载新证书 (SIGHUP = 优雅重载，不中断连接)
echo "[$(date -Iseconds)] 重载 nginx..."
docker compose kill -s HUP tinyiothub-nginx

echo "[$(date -Iseconds)] 证书续期检查完成"
