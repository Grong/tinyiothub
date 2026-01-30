#!/bin/bash
# TinyIoTHub - Docker 镜像构建和导出脚本 (Linux/macOS)
# 用于构建 ARM64 架构的 Docker 镜像并导出为 tar 文件

set -e

# 配置
API_IMAGE_NAME="tinyiothub-api"
WEB_IMAGE_NAME="tinyiothub-web"
TAG="arm64"
PLATFORM="linux/arm64"
TARGET="aarch64-unknown-linux-gnu"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
GRAY='\033[0;37m'
NC='\033[0m' # No Color

echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}TinyIoTHub - Docker 镜像构建${NC}"
echo -e "${CYAN}========================================${NC}"
echo ""

# 检查必要的工具
echo -e "${YELLOW}[1/5] 检查必要工具...${NC}"

# 检查 Docker
if ! command -v docker &> /dev/null; then
    echo -e "${RED}错误: 未找到 docker 命令${NC}"
    echo -e "${RED}请先安装 Docker: https://docs.docker.com/get-docker/${NC}"
    exit 1
fi

# 检查 Docker 是否运行
if ! docker ps &> /dev/null; then
    echo -e "${RED}错误: Docker 未运行${NC}"
    echo -e "${RED}请启动 Docker 服务${NC}"
    exit 1
fi

# 检查 pnpm
if ! command -v pnpm &> /dev/null; then
    echo -e "${RED}错误: 未找到 pnpm 命令${NC}"
    echo -e "${RED}请先安装 pnpm: npm install -g pnpm${NC}"
    exit 1
fi

echo -e "${GREEN}✓ 所有必要工具已就绪${NC}"
echo ""

# 构建后端 Docker 镜像
echo -e "${YELLOW}[2/5] 构建后端 Docker 镜像...${NC}"
echo -e "${GRAY}镜像: ${API_IMAGE_NAME}:${TAG}${NC}"

if docker build --platform $PLATFORM -t "${API_IMAGE_NAME}:${TAG}" -f Dockerfile . > /dev/null 2>&1; then
    echo -e "${GREEN}✓ 后端镜像构建完成${NC}"
else
    echo -e "${RED}错误: 后端镜像构建失败${NC}"
    exit 1
fi
echo ""

# 导出后端镜像
echo -e "${YELLOW}[3/5] 导出后端镜像...${NC}"
API_TAR="${API_IMAGE_NAME}-${TAG}.tar"

if docker save "${API_IMAGE_NAME}:${TAG}" -o $API_TAR; then
    API_SIZE=$(du -h $API_TAR | cut -f1)
    echo -e "${GREEN}✓ 后端镜像已导出: $API_TAR ($API_SIZE)${NC}"
else
    echo -e "${RED}错误: 后端镜像导出失败${NC}"
    exit 1
fi
echo ""

# 构建前端 Docker 镜像
echo -e "${YELLOW}[4/5] 构建前端 Docker 镜像...${NC}"
echo -e "${GRAY}镜像: ${WEB_IMAGE_NAME}:${TAG}${NC}"

cd web
if docker build --platform $PLATFORM -t "${WEB_IMAGE_NAME}:${TAG}" -f Dockerfile . > /dev/null 2>&1; then
    echo -e "${GREEN}✓ 前端镜像构建完成${NC}"
else
    echo -e "${RED}错误: 前端镜像构建失败${NC}"
    cd ..
    exit 1
fi
cd ..
echo ""

# 导出前端镜像
echo -e "${YELLOW}[5/5] 导出前端镜像...${NC}"
WEB_TAR="${WEB_IMAGE_NAME}-${TAG}.tar"

if docker save "${WEB_IMAGE_NAME}:${TAG}" -o $WEB_TAR; then
    WEB_SIZE=$(du -h $WEB_TAR | cut -f1)
    echo -e "${GREEN}✓ 前端镜像已导出: $WEB_TAR ($WEB_SIZE)${NC}"
else
    echo -e "${RED}错误: 前端镜像导出失败${NC}"
    exit 1
fi
echo ""

# 完成
echo -e "${CYAN}========================================${NC}"
echo -e "${GREEN}构建完成!${NC}"
echo -e "${CYAN}========================================${NC}"
echo ""
echo -e "${YELLOW}生成的文件:${NC}"
echo -e "  - $API_TAR ($API_SIZE)"
echo -e "  - $WEB_TAR ($WEB_SIZE)"
echo ""
echo -e "${YELLOW}下一步:${NC}"
echo -e "  1. 使用 hdc 传输镜像到设备:"
echo -e "${GRAY}     hdc file send $API_TAR /data/tinyiothub/${NC}"
echo -e "${GRAY}     hdc file send $WEB_TAR /data/tinyiothub/${NC}"
echo ""
echo -e "  2. 在设备上加载镜像:"
echo -e "${GRAY}     hdc shell \"cd /data/tinyiothub && docker load < $API_TAR\"${NC}"
echo -e "${GRAY}     hdc shell \"cd /data/tinyiothub && docker load < $WEB_TAR\"${NC}"
echo ""
echo -e "  3. 启动服务:"
echo -e "${GRAY}     hdc shell \"cd /data/tinyiothub && ./start-containers.sh\"${NC}"
echo ""
echo -e "${CYAN}详细部署步骤请参考: docker/README.md${NC}"
echo ""
