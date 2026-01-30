# ============================================
# Stage 1: Dependencies - 安装依赖
# ============================================
FROM node:22-alpine AS deps

RUN apk add --no-cache libc6-compat
RUN corepack enable

WORKDIR /app

COPY package.json pnpm-lock.yaml* ./
RUN corepack install && pnpm install --frozen-lockfile --prod=false

# ============================================
# Stage 2: Builder - 构建应用
# ============================================
FROM node:22-alpine AS builder

RUN corepack enable

WORKDIR /app

COPY --from=deps /app/node_modules ./node_modules
COPY . .

ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1

RUN pnpm run build

# ============================================
# Stage 3: Runner - 运行时镜像（最小化）
# ============================================
FROM node:22-alpine AS runner

WORKDIR /app

ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3000
ENV HOSTNAME="0.0.0.0"

RUN addgroup -g 1001 nodejs && \
    adduser -D -u 1001 -G nodejs nextjs && \
    apk add --no-cache wget

# 只复制必要的运行时文件
COPY --from=builder --chown=nextjs:nodejs /app/public ./public
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static

USER nextjs

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3000 || exit 1

CMD ["node", "server.js"]
