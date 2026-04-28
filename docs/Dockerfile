# syntax=docker/dockerfile:1

# ---- 构建阶段 ----
FROM node:20-alpine AS builder

WORKDIR /app

COPY package.json pnpm-lock.yaml* ./
RUN npm install -g pnpm && pnpm install --frozen-lockfile

COPY . .
RUN npx vitepress build

# ---- 运行阶段 ----
FROM nginx:alpine

COPY --from=builder /app/.vitepress/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost/ || exit 1

CMD ["nginx", "-g", "daemon off;"]
