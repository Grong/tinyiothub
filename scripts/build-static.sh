#!/bin/bash
# 构建静态单二进制版本
# 用法: ./scripts/build-static.sh [--target <target>] [--release]

set -e

TARGET=""
RELEASE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --release)
            RELEASE="--release"
            shift
            ;;
        *)
            echo "未知参数: $1"
            exit 1
            ;;
    esac
done

echo "=== 构建静态单二进制版本 ==="

# 1. 构建前端静态文件
echo -e "\n[1/3] 构建前端静态文件..."
cd web
pnpm build:static > /dev/null 2>&1
echo "✓ 前端构建完成"
cd ..

# 2. 复制静态文件到 API 目录
echo -e "\n[2/3] 准备静态文件..."
rm -rf api/web_out
cp -r web/out api/web_out
echo "✓ 静态文件已准备"

# 3. 构建后端（嵌入静态文件）
echo -e "\n[3/3] 构建后端二进制..."
cd api

BUILD_CMD="cargo build $RELEASE"
if [ -n "$TARGET" ]; then
    BUILD_CMD="$BUILD_CMD --target $TARGET"
fi

echo "执行: $BUILD_CMD"
$BUILD_CMD 2>&1 | grep -E "(Compiling|Finished|error)" || true

echo "✓ 后端构建完成"
cd ..

# 显示构建结果
echo -e "\n=== 构建完成 ==="
if [ -n "$RELEASE" ]; then
    PROFILE="release"
else
    PROFILE="debug"
fi

if [ -n "$TARGET" ]; then
    BINARY_PATH="api/target/$TARGET/$PROFILE/tinyiothub"
else
    BINARY_PATH="api/target/$PROFILE/tinyiothub"
fi

if [ -f "$BINARY_PATH" ]; then
    SIZE=$(du -h "$BINARY_PATH" | cut -f1)
    echo "二进制文件: $BINARY_PATH"
    echo "文件大小: $SIZE"
else
    echo "警告: 未找到二进制文件 $BINARY_PATH"
fi

echo -e "\n提示: 运行 '$BINARY_PATH' 启动服务"
