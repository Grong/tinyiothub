# syntax=docker/dockerfile:1

# ============================================
# Stage 1: Frontend Builder - 构建 Lit SPA 静态文件
# ============================================
FROM node:20-alpine AS frontend-builder

WORKDIR /frontend

# 安装 pnpm
RUN npm install -g pnpm

# 复制 package 文件
COPY web/package.json web/pnpm-lock.yaml* ./
RUN pnpm install --frozen-lockfile

# 复制前端源码
COPY web/ ./

# 构建静态文件（输出到 dist/ui）
RUN pnpm build

# ============================================
# Stage 2: Runtime - Nginx 服务静态文件
# ============================================
FROM nginx:alpine

# 复制构建产物
COPY --from=frontend-builder /frontend/dist/ui /usr/share/nginx/html

# 复制 Nginx SPA 配置
COPY web/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost/ || exit 1

CMD ["nginx", "-g", "daemon off;"]
