// Next.js Configuration for HarmonyOS Static Export
// 使用查询参数替代动态路由

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'export',
  
  // 禁用图片优化
  images: {
    unoptimized: true,
  },
  
  basePath: '',
  trailingSlash: true,
  
  // 生产环境配置
  productionBrowserSourceMaps: false,
  reactStrictMode: true,
  
  compiler: {
    removeConsole: { exclude: ['warn', 'error'] },
  },
  
  // 完全跳过 ESLint 和 TypeScript 检查
  eslint: {
    ignoreDuringBuilds: true,
  },
  typescript: {
    ignoreBuildErrors: true,
  },
  
  // 优化包导入
  experimental: {
    optimizePackageImports: ['@heroicons/react'],
    missingSuspenseWithCSRBailout: false,
  },
}

module.exports = nextConfig
