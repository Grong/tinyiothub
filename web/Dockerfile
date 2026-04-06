# syntax=docker/dockerfile:1

# ---- 构建阶段 ----
FROM node:20-alpine AS builder

WORKDIR /app
ENV NODE_OPTIONS="--max-old-space-size=2048"

COPY package.json pnpm-lock.yaml* ./
RUN npm install --legacy-peer-deps

COPY . .
RUN npm run build

# ---- 运行阶段 ----
FROM nginx:alpine

COPY --from=builder /dist/ui /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost/ || exit 1

CMD ["nginx", "-g", "daemon off;"]
