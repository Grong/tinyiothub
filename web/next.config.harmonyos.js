// Next.js Configuration for HarmonyOS Deployment
// 用于部署到 HarmonyOS 设备（使用 standalone 模式）

/** @type {import('next').NextConfig} */
const nextConfig = {
  // 使用 standalone 模式而不是 export（支持动态路由）
  output: 'standalone',
  
  // 禁用图片优化
  images: {
    unoptimized: true,
  },
  
  // 不使用 basePath
  basePath: '',
  
  // 生产环境配置
  productionBrowserSourceMaps: false,
  reactStrictMode: true,
  
  // 移除 console.log
  compiler: {
    removeConsole: { exclude: ['warn', 'error'] },
  },
  
  // ESLint 和 TypeScript 配置
  eslint: {
    ignoreDuringBuilds: true,  // 完全跳过 ESLint
  },
  typescript: {
    ignoreBuildErrors: true,  // 跳过类型检查
  },
  
  // 优化包导入
  experimental: {
    optimizePackageImports: [
      '@heroicons/react'
    ],
  },
}

module.exports = nextConfig
